use std::collections::HashMap;
use std::net::TcpStream;
use std::time::Duration;
use std::thread;
use std::net::ToSocketAddrs;
use std::sync::{Arc, Mutex, RwLock};
use std::io::Read;

use bytes::{BufMut, BytesMut};
use prost::Message;
use hdrhistogram::Histogram;

use metrics::{Label, KeyData};
use metrics_util::{CompositeKey, MetricKind};

mod proto {
    include!(concat!(env!("OUT_DIR"), "/event.proto.rs"));
}

#[derive(Clone)]
pub enum ClientState {
    Disconnected(Option<String>),
    Connected,
}

pub enum MetricData {
    Counter(u64),
    Gauge(f64),
    Histogram(Histogram<u64>),
}

pub struct Client {
    state: Arc<Mutex<ClientState>>,
    metrics: Arc<RwLock<HashMap<CompositeKey, MetricData>>>,
    handle: thread::JoinHandle<()>,
}

impl Client {
    pub fn new(addr: String) -> Client {
        let state = Arc::new(Mutex::new(ClientState::Disconnected(None)));
        let metrics = Arc::new(RwLock::new(HashMap::new()));
        let handle = {
            let state = state.clone();
            let metrics = metrics.clone();
            thread::spawn(move || {
                let mut runner = Runner::new(addr, state, metrics);
                runner.run();
            })
        };

        Client {
            state,
            metrics,
            handle,
        }
    }

    pub fn state(&self) -> ClientState {
        self.state.lock().unwrap().clone()
    }

    pub fn with_metrics<F, T>(&self, f: F) -> T
    where
        F: FnOnce(&HashMap<CompositeKey, MetricData>) -> T,
    {
        let handle = self.metrics.read().unwrap();
        f(&handle)
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
    metrics: Arc<RwLock<HashMap<CompositeKey, MetricData>>>,
}

impl Runner {
    pub fn new(
        addr: String,
        state: Arc<Mutex<ClientState>>,
        metrics: Arc<RwLock<HashMap<CompositeKey, MetricData>>>,
    ) -> Runner {
        Runner {
            state: RunnerState::Disconnected,
            addr,
            client_state: state,
            metrics,
        }
    }

    /*pub fn run(&mut self) {
        let mut metrics = self.metrics.write().unwrap();

        metrics.insert(("test_counter".into(), Vec::new()), MetricData::Counter(42));
        metrics.insert(
            ("test_counter_two".into(), vec!["endpoint = http".to_string()]),
            MetricData::Counter(42)
        );
        metrics.insert(("test_gauge".into(), Vec::new()), MetricData::Gauge(-666));
        
        let mut histogram = Histogram::<u64>::new(3)
            .expect("failed to create histogram");
        for i in 1..100 {
            histogram.record(i).expect("failed to record value");
        }
        metrics.insert(("test_histogram".into(), Vec::new()), MetricData::Histogram(histogram));
    }*/

    pub fn run(&mut self) {
        loop {
            let next = match self.state {
                RunnerState::Disconnected => {
                    // Just reset the client state here to be sure.
                    {
                        let mut state = self.client_state.lock().unwrap();
                        *state = ClientState::Disconnected(None);
                    }
                     
                    // Try to connect to our target and transition into Connected.
                    let addr = match self.addr.to_socket_addrs() {
                        Ok(mut addrs) => match addrs.next() {
                            Some(addr) => addr,
                            None => {
                                let mut state = self.client_state.lock().unwrap();
                                *state = ClientState::Disconnected(Some("failed to resolve specified host".to_string()));
                                break;
                            }
                        }
                        Err(_) => {
                            let mut state = self.client_state.lock().unwrap();
                            *state = ClientState::Disconnected(Some("failed to resolve specified host".to_string()));
                            break;
                        }
                    };
                    match TcpStream::connect_timeout(&addr, Duration::from_secs(3)) {
                        Ok(stream) => RunnerState::Connected(stream),
                        Err(_) => {
                            RunnerState::ErrorBackoff("error while connecting", Duration::from_secs(3))
                        }
                    }
                },
                RunnerState::ErrorBackoff(msg, dur) => {
                    {
                        let mut state = self.client_state.lock().unwrap();
                        *state = ClientState::Disconnected(Some(format!("{}, retrying in {} seconds...", msg, dur.as_secs())));
                    }
                    thread::sleep(dur);
                    RunnerState::Disconnected
                },
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
                
                        match proto::Metric::decode_length_delimited(&mut buf) {
                            Err(e) => eprintln!("decode error: {:?}", e),
                            Ok(msg) => {
                                let mut labels_raw = msg.labels.into_iter().collect::<Vec<_>>();
                                labels_raw.sort_by(|a, b| a.0.cmp(&b.0));
                                let labels = labels_raw.into_iter().map(|(k, v)| Label::new(k, v)).collect::<Vec<_>>();
                                let key_data: KeyData = (msg.name, labels).into();

                                match msg.value.expect("no metric value") {
                                    proto::metric::Value::Counter(value) => {
                                        let key = CompositeKey::new(MetricKind::Counter, key_data.into());
                                        let mut metrics = self.metrics.write().unwrap();
                                        let counter = metrics.entry(key).or_insert_with(|| MetricData::Counter(0));
                                        if let MetricData::Counter(inner) = counter {
                                            *inner += value.value;
                                        }
                                    },
                                    proto::metric::Value::Gauge(value) => {
                                        let key = CompositeKey::new(MetricKind::Gauge, key_data.into());
                                        let mut metrics = self.metrics.write().unwrap();
                                        let gauge = metrics.entry(key).or_insert_with(|| MetricData::Gauge(0.0));
                                        if let MetricData::Gauge(inner) = gauge {
                                            *inner = value.value;
                                        }
                                    },
                                    proto::metric::Value::Histogram(value) => {
                                        let key = CompositeKey::new(MetricKind::Histogram, key_data.into());
                                        let mut metrics = self.metrics.write().unwrap();
                                        let histogram = metrics.entry(key).or_insert_with(|| {
                                            let histogram = Histogram::new(3).expect("failed to create histogram");
                                            MetricData::Histogram(histogram)
                                        });

                                        if let MetricData::Histogram(inner) = histogram {
                                            inner.record(value.value).expect("failed to record value to histogram");
                                        }
                                    },
                                }
                            },
                        }
                    }

                    RunnerState::ErrorBackoff("error while observing", Duration::from_secs(3))
                }
            };
            self.state = next;
        }
    }
}