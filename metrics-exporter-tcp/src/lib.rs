//! A [`metrics`][metrics]-compatible exporter that outputs metrics to clients over TCP.
//!
//! This exporter creates a TCP server, that when connected to, will stream individual metrics to
//! the client using a Protocol Buffers encoding.
//!
//! # Backpressure
//! The exporter has configurable buffering, which allows users to trade off how many metrics they
//! want to be queued up at any given time.  This buffer limit applies both to incoming metrics, as
//! well as the individual buffers for each connected client.
//!
//! By default, the buffer limit is set at 1024 metrics.  When the incoming buffer -- metrics being
//! fed to the exported -- is full, metrics will be dropped.  If a client's buffer is full,
//! potentially due to slow network conditions or slow processing, then messages in the client's
//! buffer will be dropped in FIFO order in order to allow the exporter to continue fanning out
//! metrics to clients.
//!
//! If no buffer limit is set, then te exporter will ingest and enqueue as many metrics as possible,
//! potentially up until the point of memory exhaustion.  A buffer limit is advised for this reason,
//! even if it is many multiples of the default.
//!
//! # Encoding
//! Metrics are encoded using Protocol Buffers.  The protocol file can be found in the repository at
//! `proto/event.proto`.
//!
//! # Usage
//! The TCP exporter can be constructed by creating a [`TcpBuilder`], configuring it as needed, and
//! calling [`TcpBuilder::install`] to both spawn the TCP server as well as install the exporter
//! globally.
//!
//! If necessary, the recorder itself can be returned so that it can be composed separately, while
//! still installing the TCP server itself, by calling [`TcpBuilder::build`].
//!
//! ```
//! # use metrics_exporter_tcp::TcpBuilder;
//! # fn direct() {
//! // Install the exporter directly:
//! let builder = TcpBuilder::new();
//! builder.install().expect("failed to install TCP exporter");
//!
//! // Or install the TCP server and get the recorder:
//! let builder = TcpBuilder::new();
//! let recorder = builder.build().expect("failed to install TCP exporter");
//! # }
//! ```
//!
//! [metrics]: https://docs.rs/metrics
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg), deny(broken_intra_doc_links))]
use std::io::{self, Write};
use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::SystemTime;
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::atomic::AtomicUsize,
};

use bytes::Bytes;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use metrics::{GaugeValue, Key, Recorder, SetRecorderError, Unit};
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

use self::proto::metadata::MetricType;

enum MetricValue {
    Counter(u64),
    Gauge(GaugeValue),
    Histogram(f64),
}

enum Event {
    Metadata(Key, MetricType, Option<Unit>, Option<&'static str>),
    Metric(Key, MetricValue),
}

/// Errors that could occur while installing a TCP recorder/exporter.
#[derive(Debug)]
pub enum Error {
    /// Creating the networking event loop did not succeed.
    Io(io::Error),

    /// Installing the recorder did not succeed.
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

#[derive(Clone)]
struct State {
    client_count: Arc<AtomicUsize>,
    should_send: Arc<AtomicBool>,
    waker: Arc<Waker>,
}

impl State {
    pub fn from_waker(waker: Waker) -> State {
        State {
            client_count: Arc::new(AtomicUsize::new(0)),
            should_send: Arc::new(AtomicBool::new(false)),
            waker: Arc::new(waker),
        }
    }

    pub fn should_send(&self) -> bool {
        self.should_send.load(Ordering::Acquire)
    }

    pub fn increment_clients(&self) {
        self.client_count.fetch_add(1, Ordering::AcqRel);
        self.should_send.store(true, Ordering::Release);
    }

    pub fn decrement_clients(&self) {
        let count = self.client_count.fetch_sub(1, Ordering::AcqRel);
        if count == 1 {
            self.should_send.store(false, Ordering::Release);
        }
    }

    pub fn wake(&self) {
        let _ = self.waker.wake();
    }
}

/// A TCP recorder.
pub struct TcpRecorder {
    tx: Sender<Event>,
    state: State,
}

/// Builder for creating and installing a TCP recorder/exporter.
pub struct TcpBuilder {
    listen_addr: SocketAddr,
    buffer_size: Option<usize>,
}

impl TcpBuilder {
    /// Creates a new `TcpBuilder`.
    pub fn new() -> TcpBuilder {
        TcpBuilder {
            listen_addr: ([0, 0, 0, 0], 5000).into(),
            buffer_size: Some(1024),
        }
    }

    /// Sets the listen address.
    ///
    /// The exporter will accept connections on this address and immediately begin forwarding
    /// metrics to the client.
    ///
    /// Defaults to `0.0.0.0:5000`.
    pub fn listen_address<A>(mut self, addr: A) -> TcpBuilder
    where
        A: Into<SocketAddr>,
    {
        self.listen_addr = addr.into();
        self
    }

    /// Sets the buffer size for internal operations.
    ///
    /// The buffer size controls two operational aspects: the number of metrics processed
    /// per iteration of the event loop, and the number of buffered metrics each client
    /// can hold.
    ///
    /// This setting allows trading off responsiveness for throughput, where a smaller buffer
    /// size will ensure that metrics are pushed to clients sooner, versus a larger buffer
    /// size that allows us to push more at a time.
    ///
    /// As well, the larger the buffer, the more messages a client can temporarily hold.
    /// Clients have a circular buffer implementation so if their buffers are full, metrics
    /// will be dropped as necessary to avoid backpressure in the recorder.
    pub fn buffer_size(mut self, size: Option<usize>) -> TcpBuilder {
        self.buffer_size = size;
        self
    }

    /// Installs the recorder and exporter.
    ///
    /// An error will be returned if there's an issue with creating the TCP server or with
    /// installing the recorder as the global recorder.
    pub fn install(self) -> Result<(), Error> {
        let recorder = self.build()?;
        metrics::set_boxed_recorder(Box::new(recorder))?;
        Ok(())
    }

    /// Builds and installs the exporter, but returns the recorder.
    ///
    /// In most cases, users should prefer to use [`TcpBuilder::install`] to create and install
    /// the recorder and exporter automatically for them. If a caller is combining recorders,
    /// however, then this method allows the caller the flexibility to do so.
    pub fn build(self) -> Result<TcpRecorder, Error> {
        let buffer_size = self.buffer_size;
        let (tx, rx) = match buffer_size {
            None => unbounded(),
            Some(size) => bounded(size),
        };

        let poll = Poll::new()?;
        let waker = Waker::new(poll.registry(), WAKER)?;

        let mut listener = TcpListener::bind(self.listen_addr)?;
        poll.registry()
            .register(&mut listener, LISTENER, Interest::READABLE)?;

        let state = State::from_waker(waker);
        let recorder = TcpRecorder {
            tx,
            state: state.clone(),
        };

        thread::spawn(move || run_transport(poll, listener, rx, state, buffer_size));
        Ok(recorder)
    }
}

impl Default for TcpBuilder {
    fn default() -> Self {
        TcpBuilder::new()
    }
}

impl TcpRecorder {
    fn register_metric(
        &self,
        key: &Key,
        metric_type: MetricType,
        unit: Option<Unit>,
        description: Option<&'static str>,
    ) {
        let _ = self
            .tx
            .try_send(Event::Metadata(key.clone(), metric_type, unit, description));
        self.state.wake();
    }

    fn push_metric(&self, key: &Key, value: MetricValue) {
        if self.state.should_send() {
            let _ = self.tx.try_send(Event::Metric(key.clone(), value));
            self.state.wake();
        }
    }
}

impl Recorder for TcpRecorder {
    fn register_counter(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        self.register_metric(key, MetricType::Counter, unit, description);
    }

    fn register_gauge(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        self.register_metric(key, MetricType::Gauge, unit, description);
    }

    fn register_histogram(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        self.register_metric(key, MetricType::Histogram, unit, description);
    }

    fn increment_counter(&self, key: &Key, value: u64) {
        self.push_metric(key, MetricValue::Counter(value));
    }

    fn update_gauge(&self, key: &Key, value: GaugeValue) {
        self.push_metric(key, MetricValue::Gauge(value));
    }

    fn record_histogram(&self, key: &Key, value: f64) {
        self.push_metric(key, MetricValue::Histogram(value));
    }
}

fn run_transport(
    mut poll: Poll,
    listener: TcpListener,
    rx: Receiver<Event>,
    state: State,
    buffer_size: Option<usize>,
) {
    let buffer_limit = buffer_size.unwrap_or(std::usize::MAX);
    let mut events = Events::with_capacity(1024);
    let mut clients = HashMap::new();
    let mut clients_to_remove = Vec::new();
    let mut metadata = HashMap::new();
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

        // Technically, this is an abuse of size_hint() but Mio will return the number of events
        // for both parts of the tuple.
        trace!(events = events.iter().size_hint().0, "return from poll");

        let _pspan = trace_span!("process events");
        for event in events.iter() {
            match event.token() {
                WAKER => {
                    // Read until we hit our buffer limit or there are no more messages.
                    let _mrxspan = trace_span!("metrics in");
                    loop {
                        if buffered_pmsgs.len() >= buffer_limit {
                            // We didn't drain ourselves here, so schedule a future wake so we
                            // continue to drain remaining metrics.
                            state.wake();
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

                        match msg {
                            Event::Metadata(key, metric_type, unit, desc) => {
                                let entry = metadata
                                    .entry(key)
                                    .or_insert_with(|| (metric_type, None, None));
                                let (_, uentry, dentry) = entry;
                                *uentry = unit;
                                *dentry = desc;
                            }
                            Event::Metric(key, value) => {
                                match convert_metric_to_protobuf_encoded(key, value) {
                                    Ok(pmsg) => buffered_pmsgs.push_back(pmsg),
                                    Err(e) => error!(error = ?e, "error encoding metric"),
                                }
                            }
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
                            state.decrement_clients();
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
                            state.decrement_clients();
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
                            state.decrement_clients();
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

                                state.increment_clients();

                                // Start tracking them, and enqueue all of the metadata.
                                let metadata = generate_metadata_messages(&metadata);
                                clients
                                    .insert(token, (conn, None, metadata))
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
                                state.decrement_clients();
                            }
                        }
                    }
                }
            }
        }
    }
}

fn generate_metadata_messages(
    metadata: &HashMap<Key, (MetricType, Option<Unit>, Option<&'static str>)>,
) -> VecDeque<Bytes> {
    let mut bufs = VecDeque::new();
    for (key, (metric_type, unit, desc)) in metadata.iter() {
        let msg = convert_metadata_to_protobuf_encoded(key, *metric_type, unit.clone(), *desc)
            .expect("failed to encode metadata buffer");
        bufs.push_back(msg);
    }
    bufs
}

#[tracing::instrument(skip(wbuf, msgs))]
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
            // Zero write = client closed their connection, so remove 'em.
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

fn convert_metadata_to_protobuf_encoded(
    key: &Key,
    metric_type: MetricType,
    unit: Option<Unit>,
    desc: Option<&'static str>,
) -> Result<Bytes, EncodeError> {
    let name = key.name().to_string();
    let metadata = proto::Metadata {
        name,
        metric_type: metric_type.into(),
        unit: unit.map(|u| proto::metadata::Unit::UnitValue(u.as_str().to_owned())),
        description: desc.map(|d| proto::metadata::Description::DescriptionValue(d.to_owned())),
    };
    let event = proto::Event {
        event: Some(proto::event::Event::Metadata(metadata)),
    };

    let mut buf = Vec::new();
    event.encode_length_delimited(&mut buf)?;
    Ok(Bytes::from(buf))
}

fn convert_metric_to_protobuf_encoded(key: Key, value: MetricValue) -> Result<Bytes, EncodeError> {
    let name = key.name().to_string();
    let labels = key
        .labels()
        .map(|label| (label.key().to_owned(), label.value().to_owned()))
        .collect::<BTreeMap<_, _>>();
    let mvalue = match value {
        MetricValue::Counter(cv) => proto::metric::Value::Counter(proto::Counter { value: cv }),
        MetricValue::Gauge(gv) => match gv {
            GaugeValue::Absolute(val) => proto::metric::Value::Gauge(proto::Gauge {
                value: Some(proto::gauge::Value::Absolute(val)),
            }),
            GaugeValue::Increment(val) => proto::metric::Value::Gauge(proto::Gauge {
                value: Some(proto::gauge::Value::Increment(val)),
            }),
            GaugeValue::Decrement(val) => proto::metric::Value::Gauge(proto::Gauge {
                value: Some(proto::gauge::Value::Decrement(val)),
            }),
        },
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
    let event = proto::Event {
        event: Some(proto::event::Event::Metric(metric)),
    };

    let mut buf = Vec::new();
    event.encode_length_delimited(&mut buf)?;
    Ok(Bytes::from(buf))
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
