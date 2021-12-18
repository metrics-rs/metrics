use core::hash::Hash;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::{collections::HashMap, fmt::Debug};

use crate::{kind::MetricKind, registry::Registry, CompositeKey};

use indexmap::IndexMap;
use metrics::{Counter, Gauge, Histogram, Key, Recorder, Unit};
use ordered_float::OrderedFloat;

pub struct Snapshot(Vec<(CompositeKey, Option<Unit>, Option<&'static str>, DebugValue)>);

impl Snapshot {
    #[allow(clippy::mutable_key_type)]
    pub fn into_hashmap(
        self,
    ) -> HashMap<CompositeKey, (Option<Unit>, Option<&'static str>, DebugValue)> {
        self.0
            .into_iter()
            .map(|(k, unit, desc, value)| (k, (unit, desc, value)))
            .collect::<HashMap<_, _>>()
    }

    pub fn into_vec(self) -> Vec<(CompositeKey, Option<Unit>, Option<&'static str>, DebugValue)> {
        self.0
    }
}

/// A point-in-time value for a metric exposing raw values.
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum DebugValue {
    /// Counter.
    Counter(u64),
    /// Gauge.
    Gauge(OrderedFloat<f64>),
    /// Histogram.
    Histogram(Vec<OrderedFloat<f64>>),
}

/// Captures point-in-time snapshots of `DebuggingRecorder`.
pub struct Snapshotter {
    registry: Arc<Registry>,
    // TODO: unit/desc does actually have to be separate, because we might only describe, and not
    // register, and then iterating snapshot data to get the value for that key will fail, so gotta
    // track them separately and recombine when snapshotting
    metrics: Arc<Mutex<IndexMap<CompositeKey, (Option<Unit>, Option<&'static str>)>>>,
}

impl Snapshotter {
    /// Takes a snapshot of the recorder.
    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = Vec::new();

        let counters = self.registry.get_counter_handles();
        let gauges = self.registry.get_gauge_handles();
        let histograms = self.registry.get_histogram_handles();

        let metrics = self.metrics.lock().expect("metrics lock poisoned").clone();

        for (ck, (unit, desc)) in metrics.into_iter() {
            let value = match ck.kind() {
                MetricKind::Counter => counters
                    .get(ck.key())
                    .map(|c| DebugValue::Counter(c.load(Ordering::SeqCst))),
                MetricKind::Gauge => gauges.get(ck.key()).map(|g| {
                    let value = f64::from_bits(g.load(Ordering::SeqCst));
                    DebugValue::Gauge(value.into())
                }),
                MetricKind::Histogram => histograms.get(ck.key()).map(|h| {
                    let mut values = Vec::new();
                    h.clear_with(|xs| values.extend(xs.iter().map(|f| OrderedFloat::from(*f))));
                    DebugValue::Histogram(values)
                }),
            };

            // If there's no value for the key, that means the metric was only ever described, and
            // not registered, so don't emit it.
            if let Some(value) = value {
                snapshot.push((ck, unit, desc, value));
            }
        }

        Snapshot(snapshot)
    }
}

/// A simplistic recorder that can be installed and used for debugging or testing.
///
/// Callers can easily take snapshots of the metrics at any given time and get access
/// to the raw values.
pub struct DebuggingRecorder {
    registry: Arc<Registry>,
    metrics: Arc<Mutex<IndexMap<CompositeKey, (Option<Unit>, Option<&'static str>)>>>,
}

impl DebuggingRecorder {
    /// Creates a new `DebuggingRecorder`.
    pub fn new() -> DebuggingRecorder {
        DebuggingRecorder {
            registry: Arc::new(Registry::new()),
            metrics: Arc::new(Mutex::new(IndexMap::new())),
        }
    }

    /// Gets a `Snapshotter` attached to this recorder.
    pub fn snapshotter(&self) -> Snapshotter {
        Snapshotter {
            registry: self.registry.clone(),
            metrics: self.metrics.clone(),
        }
    }

    fn describe_metric(&self, rkey: CompositeKey, unit: Option<Unit>, desc: Option<&'static str>) {
        let mut metrics = self.metrics.lock().expect("metrics lock poisoned");
        let (uentry, dentry) = metrics.entry(rkey).or_insert((None, None));
        if unit.is_some() {
            *uentry = unit;
        }
        if desc.is_some() {
            *dentry = desc;
        }
    }

    /// Installs this recorder as the global recorder.
    pub fn install(self) -> Result<(), metrics::SetRecorderError> {
        metrics::set_boxed_recorder(Box::new(self))
    }
}

impl Recorder for DebuggingRecorder {
    fn describe_counter(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        let ckey = CompositeKey::new(MetricKind::Counter, key.clone());
        self.describe_metric(ckey, unit, description);
    }

    fn describe_gauge(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        let ckey = CompositeKey::new(MetricKind::Gauge, key.clone());
        self.describe_metric(ckey, unit, description);
    }

    fn describe_histogram(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        let ckey = CompositeKey::new(MetricKind::Histogram, key.clone());
        self.describe_metric(ckey, unit, description);
    }

    fn register_counter(&self, key: &Key) -> Counter {
        let ckey = CompositeKey::new(MetricKind::Counter, key.clone());
        self.describe_metric(ckey, None, None);
        self.registry
            .get_or_create_counter(key, |c| Counter::from_arc(c.clone()))
    }

    fn register_gauge(&self, key: &Key) -> Gauge {
        let ckey = CompositeKey::new(MetricKind::Gauge, key.clone());
        self.describe_metric(ckey, None, None);
        self.registry
            .get_or_create_gauge(key, |g| Gauge::from_arc(g.clone()))
    }

    fn register_histogram(&self, key: &Key) -> Histogram {
        let ckey = CompositeKey::new(MetricKind::Histogram, key.clone());
        self.describe_metric(ckey, None, None);
        self.registry
            .get_or_create_histogram(key, |h| Histogram::from_arc(h.clone()))
    }
}

impl Default for DebuggingRecorder {
    fn default() -> Self {
        DebuggingRecorder::new()
    }
}
