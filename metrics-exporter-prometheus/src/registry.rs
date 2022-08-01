use parking_lot::RwLock;
use portable_atomic::AtomicU64;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use metrics::{CounterFn, GaugeFn, HistogramFn, Key, Label};
use metrics_util::registry::{Generational, GenerationalStorage};
use metrics_util::AtomicBucket;
use quanta::Instant;

use crate::Distribution;

pub type GenerationalAtomicStorage = GenerationalStorage<AtomicStorage>;

/// Atomic metric storage for the prometheus exporter.
pub struct AtomicStorage;

#[derive(Clone)]
pub struct Exemplar<T> {
    pub value: T,
    pub timestamp: SystemTime,
    pub labels: Vec<Label>, // TODO this needs to be validated to its max len, 128 utf-8 characters, excluding ,="
}

pub type CounterInner = (AtomicU64, RwLock<Option<Exemplar<u64>>>);
pub type GaugeInner = (AtomicU64, RwLock<Option<Exemplar<f64>>>);
pub type HistogramInner = (AtomicBucketInstant<f64>, RwLock<HashMap<usize, Exemplar<f64>>>);

/// A counter with exemplar.clone
pub struct Counter {
    inner: Arc<CounterInner>,
}
/// A gauge with exemplar.
pub struct Gauge {
    inner: Arc<GaugeInner>,
}
/// A histogram with exemplar.
pub struct Histogram {
    inner: Arc<HistogramInner>,
    name: String,
}

impl metrics_util::registry::Storage<Key> for AtomicStorage {
    type Counter = Arc<Counter>;
    type Gauge = Arc<Gauge>;
    type Histogram = Arc<Histogram>;

    fn counter(&self, _: &Key) -> Self::Counter {
        Arc::new(Counter { inner: Arc::new((AtomicU64::new(0), RwLock::new(None))) })
    }

    fn gauge(&self, _: &Key) -> Self::Gauge {
        Arc::new(Gauge { inner: Arc::new((AtomicU64::new(0), RwLock::new(None))) })
    }

    fn histogram(&self, key: &Key) -> Self::Histogram {
        Arc::new(Histogram {
            inner: Arc::new((AtomicBucketInstant::new(), RwLock::new(HashMap::new()))),
            name: key.name().to_owned(),
        })
    }
}

/// An `AtomicBucket` newtype wrapper that tracks the time of value insertion.
pub struct AtomicBucketInstant<T> {
    inner: AtomicBucket<(T, Instant)>,
}

impl<T> AtomicBucketInstant<T> {
    fn new() -> AtomicBucketInstant<T> {
        Self { inner: AtomicBucket::new() }
    }

    pub fn clear_with<F>(&self, f: F)
    where
        F: FnMut(&[(T, Instant)]),
    {
        self.inner.clear_with(f);
    }
}

impl HistogramFn for Histogram {
    fn record(&self, value: f64) {
        let now = Instant::now();
        self.inner.0.inner.push((value, now));
    }
}

impl CounterFn for Counter {
    fn increment(&self, value: u64) {
        CounterFn::increment(&self.inner.0, value);
    }

    fn absolute(&self, value: u64) {
        CounterFn::absolute(&self.inner.0, value);
    }
}

impl GaugeFn for Gauge {
    fn increment(&self, value: f64) {
        GaugeFn::increment(&self.inner.0, value);
    }

    fn decrement(&self, value: f64) {
        GaugeFn::decrement(&self.inner.0, value);
    }

    fn set(&self, value: f64) {
        GaugeFn::set(&self.inner.0, value);
    }
}

impl Counter {
    /// Increment the counter by `value` and sets an exemplar.
    pub fn increment_with_exemplar(&self, value: u64, exemplar_labels: Vec<Label>) {
        CounterFn::increment(&self.inner.0, value);
        self.inner.1.write().replace(Exemplar {
            value,
            timestamp: SystemTime::now(),
            labels: exemplar_labels,
        });
    }

    /// Gets a reference to the inner value.
    pub fn get_inner(&self) -> &CounterInner {
        &self.inner
    }
}

impl Gauge {
    /// Increment the gauge by `value` and sets an exemplar.
    pub fn increment_with_exemplar(&self, value: f64, exemplar_labels: Vec<Label>) {
        GaugeFn::increment(&self.inner.0, value);
        self.inner.1.write().replace(Exemplar {
            value,
            timestamp: SystemTime::now(),
            labels: exemplar_labels,
        });
    }
    /// Decrement the gauge by `value` and sets an exemplar.
    pub fn decrement_with_exemplar(&self, value: f64, exemplar_labels: Vec<Label>) {
        GaugeFn::decrement(&self.inner.0, value);
        self.inner.1.write().replace(Exemplar {
            value,
            timestamp: SystemTime::now(),
            labels: exemplar_labels,
        });
    }

    /// Gets a reference to the inner value.
    pub fn get_inner(&self) -> &GaugeInner {
        &self.inner
    }
}

impl Histogram {
    /// Record `value` in histogram and sets an exemplar.
    pub fn record_with_exemplar(
        &self,
        distribution: Distribution,
        value: f64,
        exemplar_labels: Vec<Label>,
    ) {
        HistogramFn::record(self, value);

        let bucket_idx = match distribution {
            Distribution::Histogram(histogram) => histogram.bucket_index(value),
            Distribution::Summary(_, _, _) => 0, // TODO(fredr): needs fix, look into how this should work for quantiles
        };

        self.get_inner().1.write().insert(
            bucket_idx,
            Exemplar { value, timestamp: SystemTime::now(), labels: exemplar_labels },
        );
    }

    /// Gets a reference to the inner value.
    pub fn get_inner(&self) -> &HistogramInner {
        &self.inner
    }
}

impl From<Generational<Arc<Counter>>> for Counter {
    fn from(inner: Generational<Arc<Counter>>) -> Self {
        Self { inner: inner.get_inner().inner.clone() }
    }
}
impl From<Generational<Arc<Gauge>>> for Gauge {
    fn from(inner: Generational<Arc<Gauge>>) -> Self {
        Self { inner: inner.get_inner().inner.clone() }
    }
}
impl From<Generational<Arc<Histogram>>> for Histogram {
    fn from(inner: Generational<Arc<Histogram>>) -> Self {
        let inner = inner.get_inner().clone();
        Self { inner: inner.inner.clone(), name: inner.name.clone() }
    }
}
