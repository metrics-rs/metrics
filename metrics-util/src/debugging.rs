use core::hash::{Hash, Hasher};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::{handle::Handle, registry::Registry};

use indexmap::IndexMap;
use metrics::{Key, Recorder, Unit};

type UnitMap = Arc<Mutex<HashMap<DifferentiatedKey, Unit>>>;
type DescriptionMap = Arc<Mutex<HashMap<DifferentiatedKey, &'static str>>>;
type Snapshot = Vec<(
    MetricKind,
    Key,
    Option<Unit>,
    Option<&'static str>,
    DebugValue,
)>;

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
struct DifferentiatedKey(MetricKind, Key);

impl DifferentiatedKey {
    pub fn into_parts(self) -> (MetricKind, Key) {
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
    metrics: Option<Arc<Mutex<IndexMap<DifferentiatedKey, ()>>>>,
    units: UnitMap,
    descriptions: DescriptionMap,
}

impl Snapshotter {
    /// Takes a snapshot of the recorder.
    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = Vec::new();
        let handles = self.registry.get_handles();

        let collect_metric = |dkey: DifferentiatedKey,
                              handle: &Handle,
                              units: &UnitMap,
                              descs: &DescriptionMap,
                              snapshot: &mut Snapshot| {
            let unit = units
                .lock()
                .expect("units lock poisoned")
                .get(&dkey)
                .cloned();
            let desc = descs
                .lock()
                .expect("descriptions lock poisoned")
                .get(&dkey)
                .cloned();
            let (kind, key) = dkey.into_parts();
            let value = match kind {
                MetricKind::Counter => DebugValue::Counter(handle.read_counter()),
                MetricKind::Gauge => DebugValue::Gauge(handle.read_gauge()),
                MetricKind::Histogram => DebugValue::Histogram(handle.read_histogram()),
            };
            snapshot.push((kind, key, unit, desc, value));
        };

        match &self.metrics {
            Some(inner) => {
                let metrics = {
                    let metrics = inner.lock().expect("metrics lock poisoned");
                    metrics.clone()
                };

                for (dkey, _) in metrics.into_iter() {
                    if let Some(handle) = handles.get(&dkey) {
                        collect_metric(
                            dkey,
                            handle,
                            &self.units,
                            &self.descriptions,
                            &mut snapshot,
                        );
                    }
                }
            }
            None => {
                for (dkey, handle) in handles.into_iter() {
                    collect_metric(
                        dkey,
                        &handle,
                        &self.units,
                        &self.descriptions,
                        &mut snapshot,
                    );
                }
            }
        }

        snapshot
    }
}

/// A simplistic recorder that can be installed and used for debugging or testing.
///
/// Callers can easily take snapshots of the metrics at any given time and get access
/// to the raw values.
pub struct DebuggingRecorder {
    registry: Arc<Registry<DifferentiatedKey, Handle>>,
    metrics: Option<Arc<Mutex<IndexMap<DifferentiatedKey, ()>>>>,
    units: Arc<Mutex<HashMap<DifferentiatedKey, Unit>>>,
    descriptions: Arc<Mutex<HashMap<DifferentiatedKey, &'static str>>>,
}

impl DebuggingRecorder {
    /// Creates a new `DebuggingRecorder`.
    pub fn new() -> DebuggingRecorder {
        Self::with_ordering(true)
    }

    /// Creates a new `DebuggingRecorder` with ordering enabled or disabled.
    ///
    /// When ordering is enabled, any snapshotter derived from this recorder will iterate the
    /// collected metrics in order of when the metric was first observed.  If ordering is disabled,
    /// then the iteration order is undefined.
    pub fn with_ordering(ordered: bool) -> Self {
        let metrics = if ordered {
            Some(Arc::new(Mutex::new(IndexMap::new())))
        } else {
            None
        };

        DebuggingRecorder {
            registry: Arc::new(Registry::new()),
            metrics,
            units: Arc::new(Mutex::new(HashMap::new())),
            descriptions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Gets a `Snapshotter` attached to this recorder.
    pub fn snapshotter(&self) -> Snapshotter {
        Snapshotter {
            registry: self.registry.clone(),
            metrics: self.metrics.clone(),
            units: self.units.clone(),
            descriptions: self.descriptions.clone(),
        }
    }

    fn register_metric(&self, rkey: DifferentiatedKey) {
        if let Some(metrics) = &self.metrics {
            let mut metrics = metrics.lock().expect("metrics lock poisoned");
            let _ = metrics.entry(rkey.clone()).or_insert(());
        }
    }

    fn insert_unit_description(
        &self,
        rkey: DifferentiatedKey,
        unit: Option<Unit>,
        description: Option<&'static str>,
    ) {
        if let Some(unit) = unit {
            let mut units = self.units.lock().expect("units lock poisoned");
            let uentry = units.entry(rkey.clone()).or_insert_with(|| unit.clone());
            *uentry = unit;
        }
        if let Some(description) = description {
            let mut descriptions = self.descriptions.lock().expect("description lock poisoned");
            let dentry = descriptions.entry(rkey).or_insert_with(|| description);
            *dentry = description;
        }
    }

    /// Installs this recorder as the global recorder.
    pub fn install(self) -> Result<(), metrics::SetRecorderError> {
        metrics::set_boxed_recorder(Box::new(self))
    }
}

impl Recorder for DebuggingRecorder {
    fn register_counter(&self, key: Key, unit: Option<Unit>, description: Option<&'static str>) {
        let rkey = DifferentiatedKey(MetricKind::Counter, key);
        self.register_metric(rkey.clone());
        self.insert_unit_description(rkey.clone(), unit, description);
        self.registry.op(rkey, |_| {}, || Handle::counter())
    }

    fn register_gauge(&self, key: Key, unit: Option<Unit>, description: Option<&'static str>) {
        let rkey = DifferentiatedKey(MetricKind::Gauge, key);
        self.register_metric(rkey.clone());
        self.insert_unit_description(rkey.clone(), unit, description);
        self.registry.op(rkey, |_| {}, || Handle::gauge())
    }

    fn register_histogram(&self, key: Key, unit: Option<Unit>, description: Option<&'static str>) {
        let rkey = DifferentiatedKey(MetricKind::Histogram, key);
        self.register_metric(rkey.clone());
        self.insert_unit_description(rkey.clone(), unit, description);
        self.registry.op(rkey, |_| {}, || Handle::histogram())
    }

    fn increment_counter(&self, key: Key, value: u64) {
        let rkey = DifferentiatedKey(MetricKind::Counter, key);
        self.register_metric(rkey.clone());
        self.registry.op(
            rkey,
            |handle| handle.increment_counter(value),
            || Handle::counter(),
        )
    }

    fn update_gauge(&self, key: Key, value: f64) {
        let rkey = DifferentiatedKey(MetricKind::Gauge, key);
        self.register_metric(rkey.clone());
        self.registry.op(
            rkey,
            |handle| handle.update_gauge(value),
            || Handle::gauge(),
        )
    }

    fn record_histogram(&self, key: Key, value: u64) {
        let rkey = DifferentiatedKey(MetricKind::Histogram, key);
        self.register_metric(rkey.clone());
        self.registry.op(
            rkey,
            |handle| handle.record_histogram(value),
            || Handle::histogram(),
        )
    }
}
