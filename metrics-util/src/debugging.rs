//! Debugging utilities.
//!
//! While these utilities are primarily designed to help aid with testing and debugging of exporters
//! and core parts of the `metrics` ecosystem, they can be beneficial for in-process collecting of
//! metrics in some limited cases.

use std::{
    collections::HashMap,
    fmt::Debug,
    hash::Hash,
    sync::{atomic::Ordering, Arc, Mutex},
};

use crate::{
    kind::MetricKind,
    registry::{AtomicStorage, Registry},
    CompositeKey,
};

use indexmap::IndexMap;
use metrics::{
    Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SetRecorderError, SharedString,
    Unit,
};
use ordered_float::OrderedFloat;

/// A composite key name that stores both the metric key name and the metric kind.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
struct CompositeKeyName(MetricKind, KeyName);

impl CompositeKeyName {
    /// Creates a new `CompositeKeyName`.
    pub const fn new(kind: MetricKind, key_name: KeyName) -> CompositeKeyName {
        CompositeKeyName(kind, key_name)
    }
}

/// A point-in-time snapshot of all metrics in [`DebuggingRecorder`].
#[derive(Debug)]
pub struct Snapshot(Vec<(CompositeKey, Option<Unit>, Option<SharedString>, DebugValue)>);

impl Snapshot {
    /// Converts this snapshot to a mapping of metric data, keyed by the metric key itself.
    #[allow(clippy::mutable_key_type)]
    pub fn into_hashmap(
        self,
    ) -> HashMap<CompositeKey, (Option<Unit>, Option<SharedString>, DebugValue)> {
        self.0
            .into_iter()
            .map(|(k, unit, desc, value)| (k, (unit, desc, value)))
            .collect::<HashMap<_, _>>()
    }

    /// Converts this snapshot to a vector of metric data tuples.
    pub fn into_vec(self) -> Vec<(CompositeKey, Option<Unit>, Option<SharedString>, DebugValue)> {
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

#[derive(Debug)]
struct Inner {
    registry: Registry<Key, AtomicStorage>,
    seen: Mutex<IndexMap<CompositeKey, ()>>,
    metadata: Mutex<IndexMap<CompositeKeyName, (Option<Unit>, SharedString)>>,
}

impl Inner {
    fn new() -> Self {
        Self {
            registry: Registry::atomic(),
            seen: Mutex::new(IndexMap::new()),
            metadata: Mutex::new(IndexMap::new()),
        }
    }
}

/// Captures point-in-time snapshots of [`DebuggingRecorder`].
#[derive(Clone, Debug)]
pub struct Snapshotter {
    inner: Arc<Inner>,
}

impl Snapshotter {
    /// Takes a snapshot of the recorder.
    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = Vec::new();

        let counters = self.inner.registry.get_counter_handles();
        let gauges = self.inner.registry.get_gauge_handles();
        let histograms = self.inner.registry.get_histogram_handles();

        let seen = self.inner.seen.lock().expect("seen lock poisoned").clone();
        let metadata = self.inner.metadata.lock().expect("metadata lock poisoned").clone();

        for (ck, _) in seen.into_iter() {
            let value = match ck.kind() {
                MetricKind::Counter => {
                    counters.get(ck.key()).map(|c| DebugValue::Counter(c.swap(0, Ordering::SeqCst)))
                }
                MetricKind::Gauge => gauges.get(ck.key()).map(|g| {
                    let value = f64::from_bits(g.swap(0, Ordering::SeqCst));
                    DebugValue::Gauge(value.into())
                }),
                MetricKind::Histogram => histograms.get(ck.key()).map(|h| {
                    let mut values = Vec::new();
                    h.clear_with(|xs| values.extend(xs.iter().map(|f| OrderedFloat::from(*f))));
                    DebugValue::Histogram(values)
                }),
            };

            let ckn = CompositeKeyName::new(ck.kind(), ck.key().name().to_string().into());
            let (unit, desc) = metadata
                .get(&ckn)
                .map(|(u, d)| (u.to_owned(), Some(d.to_owned())))
                .unwrap_or_else(|| (None, None));

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
#[derive(Debug)]
pub struct DebuggingRecorder {
    inner: Arc<Inner>,
}

impl DebuggingRecorder {
    /// Creates a new `DebuggingRecorder`.
    pub fn new() -> DebuggingRecorder {
        DebuggingRecorder { inner: Arc::new(Inner::new()) }
    }

    /// Gets a `Snapshotter` attached to this recorder.
    pub fn snapshotter(&self) -> Snapshotter {
        Snapshotter { inner: Arc::clone(&self.inner) }
    }

    fn describe_metric(&self, rkey: CompositeKeyName, unit: Option<Unit>, desc: SharedString) {
        let mut metadata = self.inner.metadata.lock().expect("metadata lock poisoned");
        let (uentry, dentry) = metadata.entry(rkey).or_insert((None, desc.to_owned()));
        if unit.is_some() {
            *uentry = unit;
        }
        *dentry = desc;
    }

    fn track_metric(&self, ckey: CompositeKey) {
        let mut seen = self.inner.seen.lock().expect("seen lock poisoned");
        seen.insert(ckey, ());
    }

    /// Installs this recorder as the global recorder.
    pub fn install(self) -> Result<(), SetRecorderError<Self>> {
        metrics::set_global_recorder(self)
    }
}

impl Recorder for DebuggingRecorder {
    fn describe_counter(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        let ckey = CompositeKeyName::new(MetricKind::Counter, key);
        self.describe_metric(ckey, unit, description);
    }

    fn describe_gauge(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        let ckey = CompositeKeyName::new(MetricKind::Gauge, key);
        self.describe_metric(ckey, unit, description);
    }

    fn describe_histogram(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        let ckey = CompositeKeyName::new(MetricKind::Histogram, key);
        self.describe_metric(ckey, unit, description);
    }

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        let ckey = CompositeKey::new(MetricKind::Counter, key.clone());
        self.track_metric(ckey);

        self.inner.registry.get_or_create_counter(key, |c| Counter::from_arc(c.clone()))
    }

    fn register_gauge(&self, key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        let ckey = CompositeKey::new(MetricKind::Gauge, key.clone());
        self.track_metric(ckey);

        self.inner.registry.get_or_create_gauge(key, |g| Gauge::from_arc(g.clone()))
    }

    fn register_histogram(&self, key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        let ckey = CompositeKey::new(MetricKind::Histogram, key.clone());
        self.track_metric(ckey);

        self.inner.registry.get_or_create_histogram(key, |h| Histogram::from_arc(h.clone()))
    }
}

impl Default for DebuggingRecorder {
    fn default() -> Self {
        DebuggingRecorder::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use metrics::Label;

    #[test]
    fn test_debugging_recorder() {
        let recorder = DebuggingRecorder::default();
        let snapshotter = recorder.snapshotter();
        metrics::with_local_recorder(&recorder, move || {
            // We run this multiple times to validate the counters, gauges and histograms are
            // "consumed" after being snapshotted. Consumption refers to resetting the value of a
            // counter and gauge to 0 and clearing observed values from a histogram.
            for _ in 1..3 {
                let counter = metrics::counter!("test.counter", "counter.key" => "counter.value");
                let gauge_incr = metrics::gauge!("test.gauge.incr", "gauge.key" => "gauge.value");
                let gauge_set = metrics::gauge!("test.gauge.set", "gauge.key" => "gauge.value");
                let histogram =
                    metrics::histogram!("test.histogram", "histogram.key" => "histogram.value");

                counter.increment(123);
                gauge_incr.increment(456);
                gauge_set.set(654);
                histogram.record(789);

                let metrics = snapshotter.snapshot().into_vec();

                assert_eq!(
                    metrics,
                    vec![
                        (
                            CompositeKey::new(
                                MetricKind::Counter,
                                Key::from_parts(
                                    "test.counter",
                                    vec![Label::new("counter.key", "counter.value")]
                                )
                            ),
                            None,
                            None,
                            DebugValue::Counter(123)
                        ),
                        (
                            CompositeKey::new(
                                MetricKind::Gauge,
                                Key::from_parts(
                                    "test.gauge.incr",
                                    vec![Label::new("gauge.key", "gauge.value")]
                                )
                            ),
                            None,
                            None,
                            DebugValue::Gauge(OrderedFloat(456.0))
                        ),
                        (
                            CompositeKey::new(
                                MetricKind::Gauge,
                                Key::from_parts(
                                    "test.gauge.set",
                                    vec![Label::new("gauge.key", "gauge.value")]
                                )
                            ),
                            None,
                            None,
                            DebugValue::Gauge(OrderedFloat(654.0))
                        ),
                        (
                            CompositeKey::new(
                                MetricKind::Histogram,
                                Key::from_parts(
                                    "test.histogram",
                                    vec![Label::new("histogram.key", "histogram.value")]
                                )
                            ),
                            None,
                            None,
                            DebugValue::Histogram(vec![OrderedFloat(789.0)])
                        )
                    ]
                );
            }
        });
    }
}
