use std::sync::Arc;

use crate::{handle::Handle, registry::Registry};

use metrics::{Identifier, Key, Recorder};

/// Metric kinds.
#[derive(Eq, PartialEq, Hash, Clone)]
pub enum MetricKind {
    /// Counter.
    Counter,

    /// Gauge.
    Gauge,

    /// Histogram.
    Histogram,
}

#[derive(Eq, PartialEq, Hash, Clone)]
struct DifferentiatedKey(MetricKind, Key);

impl DifferentiatedKey {
    pub fn into_parts(self) -> (MetricKind, Key) {
        (self.0, self.1)
    }
}

/// A point-in-time value for a metric exposing raw values.
pub enum DebugValue {
    /// Counter.
    Counter(u64),
    /// Gauge.
    Gauge(f64),
    /// Histogram.
    Histogram(Vec<f64>),
}

/// Captures point-in-time snapshots of `DebuggingRecorder`.
pub struct Snapshotter {
    registry: Arc<Registry<DifferentiatedKey, Handle>>,
}

impl Snapshotter {
    /// Takes a snapshot of the recorder.
    pub fn snapshot(&self) -> Vec<(MetricKind, Key, DebugValue)> {
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
    fn register_counter(&self, key: Key, _description: Option<&'static str>) -> Identifier {
        let handle = Handle::counter();
        let rkey = DifferentiatedKey(MetricKind::Counter, key);
        self.registry.get_or_create_identifier(rkey, handle)
    }

    fn register_gauge(&self, key: Key, _description: Option<&'static str>) -> Identifier {
        let handle = Handle::gauge();
        let rkey = DifferentiatedKey(MetricKind::Gauge, key);
        self.registry.get_or_create_identifier(rkey, handle)
    }

    fn register_histogram(&self, key: Key, _description: Option<&'static str>) -> Identifier {
        let handle = Handle::histogram();
        let rkey = DifferentiatedKey(MetricKind::Histogram, key);
        self.registry.get_or_create_identifier(rkey, handle)
    }

    fn increment_counter(&self, id: Identifier, value: u64) {
        self.registry
            .with_handle(id, |handle| handle.increment_counter(value))
    }

    fn update_gauge(&self, id: Identifier, value: f64) {
        self.registry
            .with_handle(id, |handle| handle.update_gauge(value))
    }

    fn record_histogram(&self, id: Identifier, value: f64) {
        self.registry
            .with_handle(id, |handle| handle.record_histogram(value))
    }
}
