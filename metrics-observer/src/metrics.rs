use std::io::Read;
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;
use std::{
    collections::{BTreeMap, HashMap},
    convert::TryFrom as _,
};

use bytes::{BufMut, BytesMut};
use prost::Message;

use metrics::{Key, Label, Unit};
use metrics_util::{CompositeKey, MetricKind, Summary};

mod proto {
    include!(concat!(env!("OUT_DIR"), "/event.proto.rs"));
}

use proto::{event::Event, metadata::MetricType, metric::Operation, Event as ProstMessage};

type MetadataKey = (MetricKind, String);
type MetadataValue = (Option<Unit>, Option<String>);

#[derive(Clone)]
pub enum ClientState {
    Disconnected(Option<String>),
    Connected,
}

#[derive(Clone)]
pub enum MetricData {
    Counter(u64),
    Gauge(f64),
    Histogram(Summary),
}

pub struct Client {
    state: Arc<Mutex<ClientState>>,
    metrics: Arc<RwLock<BTreeMap<CompositeKey, MetricData>>>,
    metadata: Arc<RwLock<HashMap<MetadataKey, MetadataValue>>>,
}

impl Client {
    pub fn new(addr: String) -> Client {
        let state = Arc::new(Mutex::new(ClientState::Disconnected(None)));
        let metrics = Arc::new(RwLock::new(BTreeMap::new()));
        let metadata = Arc::new(RwLock::new(HashMap::new()));
        {
            let state = state.clone();
            let metrics = metrics.clone();
            let metadata = metadata.clone();
            thread::spawn(move || {
                let mut runner = Runner::new(addr, state, metrics, metadata);
                runner.run();
            })
        };

        Client { state, metrics, metadata }
    }

    pub fn state(&self) -> ClientState {
        self.state.lock().unwrap().clone()
    }

    pub fn get_metrics(&self) -> Vec<(CompositeKey, MetricData, Option<Unit>, Option<String>)> {
        let metrics = self.metrics.read().unwrap();
        let metadata = self.metadata.read().unwrap();

        metrics
            .iter()
            .map(|(k, v)| {
                let metakey = (k.kind(), k.key().name().to_string());
                let (unit, desc) = match metadata.get(&metakey) {
                    Some((unit, desc)) => (*unit, desc.clone()),
                    None => (None, None),
                };

                (k.clone(), v.clone(), unit, desc)
            })
            .collect()
    }
}

enum RunnerState {
    Disconnected,
    ErrorBackoff(&'static str, Duration),
    Connected(TcpStream),
}

struct Runner {
    state: RunnerState,
    addr: String,
    client_state: Arc<Mutex<ClientState>>,
    metrics: Arc<RwLock<BTreeMap<CompositeKey, MetricData>>>,
    metadata: Arc<RwLock<HashMap<MetadataKey, MetadataValue>>>,
}

impl Runner {
    pub fn new(
        addr: String,
        state: Arc<Mutex<ClientState>>,
        metrics: Arc<RwLock<BTreeMap<CompositeKey, MetricData>>>,
        metadata: Arc<RwLock<HashMap<MetadataKey, MetadataValue>>>,
    ) -> Runner {
        Runner { state: RunnerState::Disconnected, addr, client_state: state, metrics, metadata }
    }

    pub fn run(&mut self) {
        loop {
            let next = match self.state {
                RunnerState::Disconnected => {
                    // Just reset the client state here to be sure.
                    {
                        let mut state = self.client_state.lock().unwrap();
                        *state = ClientState::Disconnected(None);
                    }
                    // Resolve the target address.
                    let Ok(mut addrs) = self.addr.to_socket_addrs() else {
                        let mut state = self.client_state.lock().unwrap();
                        *state = ClientState::Disconnected(Some(
                            "failed to resolve specified host".to_string(),
                        ));
                        break;
                    };
                    // Some of the resolved addresses may be unreachable (e.g. IPv6).
                    // Pick the first one that works.
                    let maybe_stream = addrs.find_map(|addr| {
                        TcpStream::connect_timeout(&addr, Duration::from_secs(3)).ok()
                    });
                    match maybe_stream {
                        Some(stream) => RunnerState::Connected(stream),
                        None => RunnerState::ErrorBackoff(
                            "error while connecting",
                            Duration::from_secs(3),
                        ),
                    }
                }
                RunnerState::ErrorBackoff(msg, dur) => {
                    {
                        let mut state = self.client_state.lock().unwrap();
                        *state = ClientState::Disconnected(Some(format!(
                            "{}, retrying in {} seconds...",
                            msg,
                            dur.as_secs()
                        )));
                    }
                    thread::sleep(dur);
                    RunnerState::Disconnected
                }
                RunnerState::Connected(ref mut stream) => {
                    {
                        let mut state = self.client_state.lock().unwrap();
                        *state = ClientState::Connected;
                    }

                    let mut buf = BytesMut::new();
                    let mut rbuf = [0u8; 1024];

                    loop {
                        match stream.read(&mut rbuf[..]) {
                            Ok(0) => break,
                            Ok(n) => buf.put_slice(&rbuf[..n]),
                            Err(e) => eprintln!("read error: {:?}", e),
                        };

                        let message = match ProstMessage::decode_length_delimited(&mut buf) {
                            Err(e) => {
                                eprintln!("decode error: {:?}", e);
                                continue;
                            }
                            Ok(v) => v,
                        };

                        let event = match message.event {
                            Some(e) => e,
                            None => continue,
                        };

                        match event {
                            Event::Metadata(metadata) => {
                                let metric_type = MetricType::try_from(metadata.metric_type)
                                    .expect("unknown metric type over wire");
                                let metric_kind = match metric_type {
                                    MetricType::Counter => MetricKind::Counter,
                                    MetricType::Gauge => MetricKind::Gauge,
                                    MetricType::Histogram => MetricKind::Histogram,
                                };
                                let key = (metric_kind, metadata.name);
                                let mut mmap = self
                                    .metadata
                                    .write()
                                    .expect("failed to get metadata write lock");
                                let entry = mmap.entry(key).or_insert((None, None));
                                let (uentry, dentry) = entry;
                                *uentry = metadata
                                    .unit
                                    .map(|u| match u {
                                        proto::metadata::Unit::UnitValue(u) => u,
                                    })
                                    .and_then(|s| Unit::from_string(s.as_str()));
                                *dentry = metadata.description.map(|d| match d {
                                    proto::metadata::Description::DescriptionValue(ds) => ds,
                                });
                            }
                            Event::Metric(metric) => {
                                let mut labels_raw = metric.labels.into_iter().collect::<Vec<_>>();
                                labels_raw.sort_by(|a, b| a.0.cmp(&b.0));
                                let labels = labels_raw
                                    .into_iter()
                                    .map(|(k, v)| Label::new(k, v))
                                    .collect::<Vec<_>>();
                                let key_data: Key = (metric.name, labels).into();

                                match metric.operation.expect("no metric operation") {
                                    Operation::IncrementCounter(value) => {
                                        let key = CompositeKey::new(MetricKind::Counter, key_data);
                                        let mut metrics = self.metrics.write().unwrap();
                                        let counter = metrics
                                            .entry(key)
                                            .or_insert_with(|| MetricData::Counter(0));
                                        if let MetricData::Counter(inner) = counter {
                                            *inner += value;
                                        }
                                    }
                                    Operation::SetCounter(value) => {
                                        let key = CompositeKey::new(MetricKind::Counter, key_data);
                                        let mut metrics = self.metrics.write().unwrap();
                                        let counter = metrics
                                            .entry(key)
                                            .or_insert_with(|| MetricData::Counter(0));
                                        if let MetricData::Counter(inner) = counter {
                                            *inner = value;
                                        }
                                    }
                                    Operation::IncrementGauge(value) => {
                                        let key = CompositeKey::new(MetricKind::Gauge, key_data);
                                        let mut metrics = self.metrics.write().unwrap();
                                        let gauge = metrics
                                            .entry(key)
                                            .or_insert_with(|| MetricData::Gauge(0.0));
                                        if let MetricData::Gauge(inner) = gauge {
                                            *inner += value;
                                        }
                                    }
                                    Operation::DecrementGauge(value) => {
                                        let key = CompositeKey::new(MetricKind::Gauge, key_data);
                                        let mut metrics = self.metrics.write().unwrap();
                                        let gauge = metrics
                                            .entry(key)
                                            .or_insert_with(|| MetricData::Gauge(0.0));
                                        if let MetricData::Gauge(inner) = gauge {
                                            *inner -= value;
                                        }
                                    }
                                    Operation::SetGauge(value) => {
                                        let key = CompositeKey::new(MetricKind::Gauge, key_data);
                                        let mut metrics = self.metrics.write().unwrap();
                                        let gauge = metrics
                                            .entry(key)
                                            .or_insert_with(|| MetricData::Gauge(0.0));
                                        if let MetricData::Gauge(inner) = gauge {
                                            *inner = value;
                                        }
                                    }
                                    Operation::RecordHistogram(value) => {
                                        let key =
                                            CompositeKey::new(MetricKind::Histogram, key_data);
                                        let mut metrics = self.metrics.write().unwrap();
                                        let histogram = metrics.entry(key).or_insert_with(|| {
                                            let summary = Summary::with_defaults();
                                            MetricData::Histogram(summary)
                                        });

                                        if let MetricData::Histogram(inner) = histogram {
                                            inner.add(value);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    RunnerState::ErrorBackoff("error while observing", Duration::from_secs(3))
                }
            };
            self.state = next;
        }
    }
}
