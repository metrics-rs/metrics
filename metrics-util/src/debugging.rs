use core::hash::Hash;
use std::sync::{Arc, Mutex};
use std::{collections::HashMap, fmt::Debug};

use crate::{handle::Handle, kind::MetricKind, registry::Registry, CompositeKey};

use indexmap::IndexMap;
use metrics::{GaugeValue, Key, Recorder, Unit};
use ordered_float::OrderedFloat;

type UnitMap = Arc<Mutex<HashMap<CompositeKey, Unit>>>;
type DescriptionMap = Arc<Mutex<HashMap<CompositeKey, &'static str>>>;
type Snapshot = Vec<(CompositeKey, Option<Unit>, Option<&'static str>, DebugValue)>;

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
    registry: Arc<Registry<Key, Handle>>,
    metrics: Option<Arc<Mutex<IndexMap<CompositeKey, ()>>>>,
    units: UnitMap,
    descs: DescriptionMap,
}

impl Snapshotter {
    /// Takes a snapshot of the recorder.
    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = Vec::new();
        let handles = self.registry.get_handles();

        let collect_metric = |kind: MetricKind,
                              key: Key,
                              handle: &Handle,
                              units: &UnitMap,
                              descs: &DescriptionMap,
                              snapshot: &mut Snapshot| {
            let ckey = CompositeKey::new(kind, key);
            let unit = units
                .lock()
                .expect("units lock poisoned")
                .get(&ckey)
                .cloned();
            let desc = descs
                .lock()
                .expect("descriptions lock poisoned")
                .get(&ckey)
                .cloned();
            let value = match kind {
                MetricKind::Counter => DebugValue::Counter(handle.read_counter()),
                MetricKind::Gauge => DebugValue::Gauge(handle.read_gauge().into()),
                MetricKind::Histogram => {
                    let mapped = handle
                        .read_histogram()
                        .into_iter()
                        .map(Into::into)
                        .collect();
                    DebugValue::Histogram(mapped)
                }
            };
            snapshot.push((ckey, unit, desc, value));
        };

        match &self.metrics {
            Some(inner) => {
                let metrics = {
                    let metrics = inner.lock().expect("metrics lock poisoned");
                    metrics.clone()
                };

                for (dk, _) in metrics.into_iter() {
                    let key = dk.into_parts();
                    if let Some((_, h)) = handles.get(&key) {
                        collect_metric(key.0, key.1, h, &self.units, &self.descs, &mut snapshot);
                    }
                }
            }
            None => {
                for ((kind, key), (_, h)) in handles.into_iter() {
                    collect_metric(kind, key, &h, &self.units, &self.descs, &mut snapshot);
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
    registry: Arc<Registry<Key, Handle>>,
    metrics: Option<Arc<Mutex<IndexMap<CompositeKey, ()>>>>,
    units: Arc<Mutex<HashMap<CompositeKey, Unit>>>,
    descs: Arc<Mutex<HashMap<CompositeKey, &'static str>>>,
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
            descs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Gets a `Snapshotter` attached to this recorder.
    pub fn snapshotter(&self) -> Snapshotter {
        Snapshotter {
            registry: self.registry.clone(),
            metrics: self.metrics.clone(),
            units: self.units.clone(),
            descs: self.descs.clone(),
        }
    }

    fn register_metric(&self, rkey: CompositeKey) {
        if let Some(metrics) = &self.metrics {
            let mut metrics = metrics.lock().expect("metrics lock poisoned");
            let _ = metrics.entry(rkey).or_insert(());
        }
    }

    fn insert_unit_description(
        &self,
        rkey: CompositeKey,
        unit: Option<Unit>,
        desc: Option<&'static str>,
    ) {
        if let Some(unit) = unit {
            let mut units = self.units.lock().expect("units lock poisoned");
            let uentry = units.entry(rkey.clone()).or_insert_with(|| unit.clone());
            *uentry = unit;
        }
        if let Some(desc) = desc {
            let mut descs = self.descs.lock().expect("description lock poisoned");
            let dentry = descs.entry(rkey).or_insert_with(|| desc);
            *dentry = desc;
        }
    }

    /// Installs this recorder as the global recorder.
    pub fn install(self) -> Result<(), metrics::SetRecorderError> {
        metrics::set_boxed_recorder(Box::new(self))
    }
}

impl Recorder for DebuggingRecorder {
    fn register_counter(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        let rkey = CompositeKey::new(MetricKind::Counter, key.clone());
        self.register_metric(rkey.clone());
        self.insert_unit_description(rkey.clone(), unit, description);
        self.registry.op(MetricKind::Counter, key, |_| {}, Handle::counter)
    }

    fn register_gauge(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        let rkey = CompositeKey::new(MetricKind::Gauge, key.clone());
        self.register_metric(rkey.clone());
        self.insert_unit_description(rkey.clone(), unit, description);
        self.registry.op(MetricKind::Gauge, key, |_| {}, Handle::gauge)
    }

    fn register_histogram(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        let rkey = CompositeKey::new(MetricKind::Histogram, key.clone());
        self.register_metric(rkey.clone());
        self.insert_unit_description(rkey.clone(), unit, description);
        self.registry.op(MetricKind::Histogram, key, |_| {}, Handle::histogram)
    }

    fn increment_counter(&self, key: &Key, value: u64) {
        let rkey = CompositeKey::new(MetricKind::Counter, key.clone());
        self.register_metric(rkey.clone());
        self.registry.op(
            MetricKind::Counter,
            key,
            |handle| handle.increment_counter(value),
            Handle::counter,
        )
    }

    fn update_gauge(&self, key: &Key, value: GaugeValue) {
        let rkey = CompositeKey::new(MetricKind::Gauge, key.clone());
        self.register_metric(rkey.clone());
        self.registry.op(
            MetricKind::Gauge,
            key,
            |handle| handle.update_gauge(value),
            Handle::gauge
        )
    }

    fn record_histogram(&self, key: &Key, value: f64) {
        let rkey = CompositeKey::new(MetricKind::Histogram, key.clone());
        self.register_metric(rkey.clone());
        self.registry.op(
            MetricKind::Histogram,
            key,
            |handle| handle.record_histogram(value),
            Handle::histogram,
        )
    }
}

impl Default for DebuggingRecorder {
    fn default() -> Self {
        DebuggingRecorder::new()
    }
}
