use std::sync::Arc;

use metrics::{atomics::AtomicU64, CounterFn, GaugeFn, HistogramFn};

use crate::AtomicBucket;

/// Defines the underlying storage for metrics as well as how to create them.
pub trait Storage<K> {
    /// The type used for counters.
    type Counter: CounterFn + Clone;

    /// The type used for gauges.
    type Gauge: GaugeFn + Clone;

    /// The type used for histograms.
    type Histogram: HistogramFn + Clone;

    /// Creates an empty counter.
    fn counter(&self, key: &K) -> Self::Counter;

    /// Creates an empty gauge.
    fn gauge(&self, key: &K) -> Self::Gauge;

    /// Creates an empty histogram.
    fn histogram(&self, key: &K) -> Self::Histogram;
}

/// Atomic metric storage.
///
/// Utilizes atomics for storing the value(s) of a given metric.  Shared access to the actual atomic
/// is handling via `Arc`.
pub struct AtomicStorage;

impl<K> Storage<K> for AtomicStorage {
    type Counter = Arc<AtomicU64>;
    type Gauge = Arc<AtomicU64>;
    type Histogram = Arc<AtomicBucket<f64>>;

    fn counter(&self, _: &K) -> Self::Counter {
        Arc::new(AtomicU64::new(0))
    }

    fn gauge(&self, _: &K) -> Self::Gauge {
        Arc::new(AtomicU64::new(0))
    }

    fn histogram(&self, _: &K) -> Self::Histogram {
        Arc::new(AtomicBucket::new())
    }
}
