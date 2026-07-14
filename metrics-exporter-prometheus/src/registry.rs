use std::sync::Arc;

use metrics::{atomics::AtomicU64, HistogramFn};
use metrics_util::{registry::GenerationalStorage, storage::AtomicBucket};
use quanta::Instant;

pub type GenerationalAtomicStorage = GenerationalStorage<AtomicStorage>;

/// Atomic metric storage for the prometheus exporter.
#[derive(Debug)]
pub struct AtomicStorage;

impl<K> metrics_util::registry::Storage<K> for AtomicStorage {
    type Counter = Arc<AtomicU64>;
    type Gauge = Arc<AtomicU64>;
    type Histogram = Arc<AtomicBucketInstant<f64>>;

    fn counter(&self, _: &K) -> Self::Counter {
        Arc::new(AtomicU64::new(0))
    }

    fn gauge(&self, _: &K) -> Self::Gauge {
        Arc::new(AtomicU64::new(0))
    }

    fn histogram(&self, _: &K) -> Self::Histogram {
        Arc::new(AtomicBucketInstant::new())
    }
}

/// An `AtomicBucket` newtype wrapper that tracks the time of value insertion,
/// and the number of times each value was observed.
///
/// Entries are `(value, count, insertion time)`: a plain `record` stores a
/// count of 1, while `record_many` stores its full count in a single entry so
/// that recording a value N times is O(1) in both time and buffered memory.
#[derive(Debug)]
pub struct AtomicBucketInstant<T> {
    inner: AtomicBucket<(T, usize, Instant)>,
}

impl<T> AtomicBucketInstant<T> {
    fn new() -> AtomicBucketInstant<T> {
        Self { inner: AtomicBucket::new() }
    }

    pub fn clear_with<F>(&self, f: F)
    where
        F: FnMut(&[(T, usize, Instant)]),
    {
        self.inner.clear_with(f);
    }
}

impl HistogramFn for AtomicBucketInstant<f64> {
    fn record(&self, value: f64) {
        let now = Instant::now();
        self.inner.push((value, 1, now));
    }

    fn record_many(&self, value: f64, count: usize) {
        if count == 0 {
            return;
        }
        let now = Instant::now();
        self.inner.push((value, count, now));
    }
}

#[cfg(test)]
mod tests {
    use super::AtomicBucketInstant;
    use metrics::HistogramFn;

    #[test]
    fn record_many_buffers_a_single_entry() {
        let bucket = AtomicBucketInstant::new();
        bucket.record_many(3.0, 1_000_000);
        bucket.record(2.0);
        bucket.record_many(9.0, 0); // no-op: buffers nothing

        let mut entries = Vec::new();
        bucket.clear_with(|xs| entries.extend_from_slice(xs));

        // The whole point of the counted entry: a million-count record is ONE
        // buffered sample, not a million.
        assert_eq!(entries.len(), 2);
        assert_eq!((entries[0].0, entries[0].1), (3.0, 1_000_000));
        assert_eq!((entries[1].0, entries[1].1), (2.0, 1));
    }
}
