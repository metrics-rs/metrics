use std::{hash::Hash, hash::Hasher, sync::Arc};

use crate::{handle::Handle, registry::Registry};

use metrics::{KeyRef, Recorder};

/// Metric kinds.
#[derive(Debug, Eq, PartialEq, Hash, Clone, Copy, Ord, PartialOrd)]
pub enum MetricKind {
    /// Counter.
    Counter,

    /// Gauge.
    Gauge,

    /// Histogram.
    Histogram,
}

#[derive(Eq, PartialEq, Hash, Clone)]
struct DifferentiatedKey(MetricKind, KeyRef);

impl DifferentiatedKey {
    pub fn into_parts(self) -> (MetricKind, KeyRef) {
        (self.0, self.1)
    }
}

/// A point-in-time value for a metric exposing raw values.
#[derive(Debug, PartialEq)]
pub enum DebugValue {
    /// Counter.
    Counter(u64),
    /// Gauge.
    Gauge(f64),
    /// Histogram.
    Histogram(Vec<u64>),
}

// We don't care that much about total equality nuances here.
impl Eq for DebugValue {}

impl Hash for DebugValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Counter(val) => val.hash(state),
            Self::Gauge(val) => {
                // Whatever works, we don't really care in here...
                if val.is_normal() {
                    val.to_ne_bytes().hash(state)
                } else {
                    0f64.to_ne_bytes().hash(state)
                }
            }
            Self::Histogram(val) => val.hash(state),
        }
    }
}

/// Captures point-in-time snapshots of `DebuggingRecorder`.
pub struct Snapshotter {
    registry: Arc<Registry<DifferentiatedKey, Handle>>,
}

impl Snapshotter {
    /// Takes a snapshot of the recorder.
    pub fn snapshot(&self) -> Vec<(MetricKind, KeyRef, DebugValue)> {
        let mut metrics = Vec::new();
        let handles = self.registry.get_handles();
        for (dkey, handle) in handles {
            let (kind, key) = dkey.into_parts();
            let value = match kind {
                MetricKind::Counter => DebugValue::Counter(handle.read_counter()),
                MetricKind::Gauge => DebugValue::Gauge(handle.read_gauge()),
                MetricKind::Histogram => DebugValue::Histogram(handle.read_histogram()),
            };
            metrics.push((kind, key, value));
        }
        metrics
    }
}

/// A simplistic recorder that can be installed and used for debugging or testing.
///
/// Callers can easily take snapshots of the metrics at any given time and get access
/// to the raw values.
pub struct DebuggingRecorder {
    registry: Arc<Registry<DifferentiatedKey, Handle>>,
}

impl DebuggingRecorder {
    /// Creates a new `DebuggingRecorder`.
    pub fn new() -> DebuggingRecorder {
        DebuggingRecorder {
            registry: Arc::new(Registry::new()),
        }
    }

    /// Gets a `Snapshotter` attached to this recorder.
    pub fn snapshotter(&self) -> Snapshotter {
        Snapshotter {
            registry: self.registry.clone(),
        }
    }

    /// Installs this recorder as the global recorder.
    pub fn install(self) -> Result<(), metrics::SetRecorderError> {
        metrics::set_boxed_recorder(Box::new(self))
    }
}

impl Recorder for DebuggingRecorder {
    fn register_counter(&self, key: KeyRef, _description: Option<&'static str>) {
        let rkey = DifferentiatedKey(MetricKind::Counter, key);
        self.registry
            .get_or_create_identifier(rkey, |_| Handle::counter());
    }

    fn register_gauge(&self, key: KeyRef, _description: Option<&'static str>) {
        let rkey = DifferentiatedKey(MetricKind::Gauge, key);
        self.registry
            .get_or_create_identifier(rkey, |_| Handle::gauge());
    }

    fn register_histogram(&self, key: KeyRef, _description: Option<&'static str>) {
        let rkey = DifferentiatedKey(MetricKind::Histogram, key);
        self.registry
            .get_or_create_identifier(rkey, |_| Handle::histogram());
    }

    fn increment_counter(&self, key: KeyRef, value: u64) {
        let rkey = DifferentiatedKey(MetricKind::Counter, key);
        let id = self
            .registry
            .get_or_create_identifier(rkey, |_| Handle::counter());
        self.registry
            .with_handle(id, |handle| handle.increment_counter(value));
    }

    fn update_gauge(&self, key: KeyRef, value: f64) {
        let rkey = DifferentiatedKey(MetricKind::Gauge, key);
        let id = self
            .registry
            .get_or_create_identifier(rkey, |_| Handle::gauge());
        self.registry
            .with_handle(id, |handle| handle.update_gauge(value));
    }

    fn record_histogram(&self, key: KeyRef, value: u64) {
        let rkey = DifferentiatedKey(MetricKind::Histogram, key);
        let id = self
            .registry
            .get_or_create_identifier(rkey, |_| Handle::histogram());
        self.registry
            .with_handle(id, |handle| handle.record_histogram(value));
    }
}
