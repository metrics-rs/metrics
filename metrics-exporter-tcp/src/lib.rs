use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::{self, Write};
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;
use std::time::SystemTime;

use bytes::Bytes;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use metrics::{Identifier, Key, Recorder, SetRecorderError};
use metrics_util::Registry;
use mio::{
    net::{TcpListener, TcpStream},
    Events, Interest, Poll, Token, Waker,
};
use prost::{EncodeError, Message};
use tracing::{error, trace, trace_span};

const WAKER: Token = Token(0);
const LISTENER: Token = Token(1);
const START_TOKEN: Token = Token(2);
const CLIENT_INTEREST: Interest = Interest::READABLE.add(Interest::WRITABLE);

mod proto {
    include!(concat!(env!("OUT_DIR"), "/event.proto.rs"));
}

type TcpRegistry = Registry<CompositeKey, CompositeKey>;

#[derive(Eq, PartialEq, Hash, Clone)]
enum MetricKind {
    Counter,
    Gauge,
    Histogram,
}

enum MetricValue {
    Counter(u64),
    Gauge(f64),
    Histogram(f64),
}

#[derive(Eq, PartialEq, Hash, Clone)]
struct CompositeKey(MetricKind, Key);

impl CompositeKey {
    pub fn key(&self) -> &Key {
        &self.1
    }
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Recorder(SetRecorderError),
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<SetRecorderError> for Error {
    fn from(e: SetRecorderError) -> Self {
        Error::Recorder(e)
    }
}

struct TcpRecorder {
    registry: Arc<TcpRegistry>,
    tx: Sender<(Identifier, MetricValue)>,
    waker: Waker,
}

pub struct TcpBuilder {
    addr: SocketAddr,
    buffer_size: Option<usize>,
}

impl TcpBuilder {
    pub fn new() -> TcpBuilder {
        TcpBuilder {
            addr: ([127, 0, 0, 1], 5000).into(),
            buffer_size: Some(1024),
        }
    }

    pub fn bind_address<A>(mut self, addr: A) -> TcpBuilder
    where
        A: Into<SocketAddr>,
    {
        self.addr = addr.into();
        self
    }

    pub fn buffer_size(mut self, size: Option<usize>) -> TcpBuilder {
        self.buffer_size = size;
        self
    }

    pub fn install(self) -> Result<(), Error> {
        let buffer_size = self.buffer_size;
        let (tx, rx) = match buffer_size {
            None => unbounded(),
            Some(size) => bounded(size),
        };

        let poll = Poll::new()?;
        let waker = Waker::new(poll.registry(), WAKER)?;

        let mut listener = TcpListener::bind(self.addr)?;
        poll.registry()
            .register(&mut listener, LISTENER, Interest::READABLE)?;

        let registry = Arc::new(Registry::new());

        let recorder = TcpRecorder {
            registry: Arc::clone(&registry),
            tx,
            waker,
        };
        metrics::set_boxed_recorder(Box::new(recorder))?;

        thread::spawn(move || run_transport(registry, poll, listener, rx, buffer_size));
        Ok(())
    }
}

impl TcpRecorder {
    fn register_metric(&self, kind: MetricKind, key: Key) -> Identifier {
        let ckey = CompositeKey(kind, key);
        self.registry.get_or_create_identifier(ckey, |k| k.clone())
    }

    fn push_metric(&self, id: Identifier, value: MetricValue) {
        let _ = self.tx.try_send((id, value));
        let _ = self.waker.wake();
    }
}

impl Recorder for TcpRecorder {
    fn register_counter(&self, key: Key, _description: Option<&'static str>) -> Identifier {
        self.register_metric(MetricKind::Counter, key)
    }

    fn register_gauge(&self, key: Key, _description: Option<&'static str>) -> Identifier {
        self.register_metric(MetricKind::Gauge, key)
    }

    fn register_histogram(&self, key: Key, _description: Option<&'static str>) -> Identifier {
        self.register_metric(MetricKind::Histogram, key)
    }

    fn increment_counter(&self, id: Identifier, value: u64) {
        self.push_metric(id, MetricValue::Counter(value));
    }

    fn update_gauge(&self, id: Identifier, value: f64) {
        self.push_metric(id, MetricValue::Gauge(value));
    }

    fn record_histogram(&self, id: Identifier, value: f64) {
        self.push_metric(id, MetricValue::Histogram(value));
    }
}

fn run_transport(
    registry: Arc<TcpRegistry>,
    mut poll: Poll,
    listener: TcpListener,
    rx: Receiver<(Identifier, MetricValue)>,
    buffer_size: Option<usize>,
) {
    let buffer_limit = buffer_size.unwrap_or(std::usize::MAX);
    let mut events = Events::with_capacity(1024);
    let mut clients = HashMap::new();
    let mut clients_to_remove = Vec::new();
    let mut next_token = START_TOKEN;
    let mut buffered_pmsgs = VecDeque::with_capacity(buffer_limit);

    loop {
        let _span = trace_span!("transport");

        // Poll until we get something.  All events -- metrics wake-ups and network I/O -- flow
        // through here so we can block without issue.
        let _evspan = trace_span!("event loop");
        if let Err(e) = poll.poll(&mut events, None) {
            error!(error = %e, "error during poll");
            continue;
        }
        drop(_evspan);

        // undo when mio can show poll event count
        // trace!("return from poll, events = events.len());

        let _pspan = trace_span!("process events");
        for event in events.iter() {
            match event.token() {
                WAKER => {
                    // Read until we hit our buffer limit or there are no more messages.
                    let _mrxspan = trace_span!("metrics in");
                    loop {
                        if buffered_pmsgs.len() >= buffer_limit {
                            break;
                        }

                        let msg = match rx.try_recv() {
                            Ok(msg) => msg,
                            Err(e) if e.is_empty() => {
                                trace!("metric rx drained");
                                break;
                            }
                            // If our sender is dead, we can't do anything else, so just return.
                            Err(_) => return,
                        };
                        match convert_metric_to_protobuf_encoded(&registry, msg.0, msg.1) {
                            Some(Ok(pmsg)) => buffered_pmsgs.push_back(pmsg),
                            Some(Err(e)) => error!(error = ?e, "error encoding metric"),
                            None => error!(metric_id = msg.0.to_usize(), "unknown metric"),
                        }
                    }
                    drop(_mrxspan);

                    if buffered_pmsgs.is_empty() {
                        trace!("woken for metrics but no pmsgs buffered");
                        continue;
                    }

                    // Now fan out each of these items to each client.
                    for (token, (conn, wbuf, msgs)) in clients.iter_mut() {
                        // Before we potentially do any draining, try and drive the connection to
                        // make sure space is freed up as much as possible.
                        let done = drive_connection(conn, wbuf, msgs);
                        if done {
                            clients_to_remove.push(*token);
                            continue;
                        }

                        // With the encoded metrics, we push them into each client's internal
                        // list.  We try to write as many of those buffers as possible to the
                        // client before being told to back off.  If we encounter a partial write
                        // of a buffer, we store the remaining of that message in a special field
                        // so that we don't write incomplete metrics to the client.
                        //
                        // If there are more messages to hand off to a client than the client's
                        // internal list has room for, we remove as many as needed to do so.  This
                        // means we prioritize sending newer metrics if connections are backed up.
                        let available = if msgs.len() < buffer_limit {
                            buffer_limit - msgs.len()
                        } else {
                            0
                        };
                        let to_drain = buffered_pmsgs.len().saturating_sub(available);
                        let _ = msgs.drain(0..to_drain);
                        msgs.extend(buffered_pmsgs.iter().take(buffer_limit).cloned());

                        let done = drive_connection(conn, wbuf, msgs);
                        if done {
                            clients_to_remove.push(*token);
                        }
                    }

                    // We've pushed each metric into each client's internal list, so we can clear
                    // ourselves and continue on.
                    buffered_pmsgs.clear();

                    // Remove any clients that were done.
                    for token in clients_to_remove.drain(..) {
                        if let Some((conn, _, _)) = clients.get_mut(&token) {
                            trace!(?conn, ?token, "removing client");
                            clients.remove(&token);
                        }
                    }
                }
                LISTENER => {
                    // Accept as many new connections as we can.
                    loop {
                        match listener.accept() {
                            Ok((mut conn, _)) => {
                                // Get our client's token and register the connection.
                                let token = next(&mut next_token);
                                poll.registry()
                                    .register(&mut conn, token, CLIENT_INTEREST)
                                    .expect("failed to register interest for client connection");

                                // Start tracking them.
                                clients
                                    .insert(token, (conn, None, VecDeque::new()))
                                    .ok_or(())
                                    .expect_err("client mapped to existing token!");
                            }
                            Err(ref e) if would_block(e) => break,
                            Err(e) => {
                                error!("caught error while accepting client connections: {:?}", e);
                                return;
                            }
                        }
                    }
                }
                token => {
                    if event.is_writable() {
                        if let Some((conn, wbuf, msgs)) = clients.get_mut(&token) {
                            let done = drive_connection(conn, wbuf, msgs);
                            if done {
                                trace!(?conn, ?token, "removing client");
                                clients.remove(&token);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[tracing::instrument]
fn drive_connection(
    conn: &mut TcpStream,
    wbuf: &mut Option<Bytes>,
    msgs: &mut VecDeque<Bytes>,
) -> bool {
    trace!(?conn, "driving client");
    loop {
        let mut buf = match wbuf.take() {
            // Send the leftover buffer first, if we have one.
            Some(buf) => buf,
            None => match msgs.pop_front() {
                Some(msg) => msg,
                None => {
                    trace!("client write queue drained");
                    return false;
                }
            },
        };

        match conn.write(&buf) {
            // Zero write = client closedd their connection, so remove 'em.
            Ok(0) => {
                trace!(?conn, "zero write, closing client");
                return true;
            }
            Ok(n) if n < buf.len() => {
                // We sent part of the buffer, but not everything.  Keep track of the remaining
                // chunk of the buffer.  TODO: do we need to reregister ourselves to track writable
                // status??
                let remaining = buf.split_off(n);
                trace!(
                    ?conn,
                    written = n,
                    remaining = remaining.len(),
                    "partial write"
                );
                wbuf.replace(remaining);
                return false;
            }
            Ok(_) => continue,
            Err(ref e) if would_block(e) => return false,
            Err(ref e) if interrupted(e) => return drive_connection(conn, wbuf, msgs),
            Err(e) => {
                error!(?conn, error = %e, "write failed");
                return true;
            }
        }
    }
}

fn convert_metric_to_protobuf_encoded(
    registry: &Arc<TcpRegistry>,
    id: Identifier,
    value: MetricValue,
) -> Option<Result<Bytes, EncodeError>> {
    registry.with_handle(id, |ckey| {
        let name = ckey.key().name().to_string();
        let labels = ckey
            .key()
            .labels()
            .map(|label| (label.key().to_string(), label.value().to_string()))
            .collect::<BTreeMap<_, _>>();
        let mvalue = match value {
            MetricValue::Counter(cv) => proto::metric::Value::Counter(proto::Counter { value: cv }),
            MetricValue::Gauge(gv) => proto::metric::Value::Gauge(proto::Gauge { value: gv }),
            MetricValue::Histogram(hv) => {
                proto::metric::Value::Histogram(proto::Histogram { value: hv })
            }
        };

        let now: prost_types::Timestamp = SystemTime::now().into();
        let metric = proto::Metric {
            name,
            labels,
            timestamp: Some(now),
            value: Some(mvalue),
        };

        let mut buf = Vec::new();
        metric.encode_length_delimited(&mut buf)?;
        Ok(Bytes::from(buf))
    })
}

fn next(current: &mut Token) -> Token {
    let next = current.0;
    current.0 += 1;
    Token(next)
}

fn would_block(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::WouldBlock
}

fn interrupted(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::Interrupted
}
