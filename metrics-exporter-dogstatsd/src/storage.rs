use std::{
    slice::Iter,
    sync::{
        atomic::{
            AtomicBool, AtomicU64,
            Ordering::{AcqRel, Acquire, Relaxed, Release},
        },
        Arc,
    },
};

use metrics::{CounterFn, GaugeFn, HistogramFn, Key};
use metrics_util::{
    registry::Storage,
    storage::{
        reservoir::{AtomicSamplingReservoir, Drain},
        AtomicBucket,
    },
};

pub(crate) struct AtomicCounter {
    is_absolute: AtomicBool,
    last: AtomicU64,
    current: AtomicU64,
    updates: AtomicU64,
}

impl AtomicCounter {
    /// Creates a new `AtomicCounter`.
    fn new() -> Self {
        Self {
            is_absolute: AtomicBool::new(false),
            last: AtomicU64::new(0),
            current: AtomicU64::new(0),
            updates: AtomicU64::new(0),
        }
    }

    /// Flushes the current counter value, returning the delta of the counter value, and the number of updates, since the last flush.
    pub fn flush(&self) -> (u64, u64) {
        let current = self.current.load(Acquire);
        let last = self.last.swap(current, AcqRel);
        let delta = current.wrapping_sub(last);
        let updates = self.updates.swap(0, AcqRel);

        (delta, updates)
    }
}

impl CounterFn for AtomicCounter {
    fn increment(&self, value: u64) {
        self.is_absolute.store(false, Release);
        self.current.fetch_add(value, Relaxed);
        self.updates.fetch_add(1, Relaxed);
    }

    fn absolute(&self, value: u64) {
        // Ensure the counter is in absolute mode, and if it wasn't already, reset `last` to `value` to give ourselves a
        // consistent starting point when flushing. This ensures that we only start flushing deltas once we've gotten
        // two consecutive absolute values, since otherwise we might be calculating a delta between a `last` of 0 and a
        // very large `current` value.
        if !self.is_absolute.swap(true, Release) {
            self.last.store(value, Release);
        }

        self.current.store(value, Release);
        self.updates.fetch_add(1, Relaxed);
    }
}

pub(crate) struct AtomicGauge {
    inner: AtomicU64,
    updates: AtomicU64,
}

impl AtomicGauge {
    /// Creates a new `AtomicGauge`.
    fn new() -> Self {
        Self { inner: AtomicU64::new(0.0f64.to_bits()), updates: AtomicU64::new(0) }
    }

    /// Flushes the current gauge value and the number of updates since the last flush.
    pub fn flush(&self) -> (f64, u64) {
        let current = f64::from_bits(self.inner.load(Acquire));
        let updates = self.updates.swap(0, AcqRel);

        (current, updates)
    }
}

impl GaugeFn for AtomicGauge {
    fn increment(&self, value: f64) {
        self.inner
            .fetch_update(AcqRel, Relaxed, |current| {
                let new = f64::from_bits(current) + value;
                Some(f64::to_bits(new))
            })
            .expect("should never fail to update gauge");
        self.updates.fetch_add(1, Relaxed);
    }

    fn decrement(&self, value: f64) {
        self.inner
            .fetch_update(AcqRel, Relaxed, |current| {
                let new = f64::from_bits(current) - value;
                Some(f64::to_bits(new))
            })
            .expect("should never fail to update gauge");
        self.updates.fetch_add(1, Relaxed);
    }

    fn set(&self, value: f64) {
        self.inner.store(value.to_bits(), Release);
        self.updates.fetch_add(1, Relaxed);
    }
}

pub(crate) enum AtomicHistogram {
    Raw(AtomicBucket<f64>),
    Sampled(AtomicSamplingReservoir),
}

impl AtomicHistogram {
    /// Creates a new `AtomicHistogram` based on the given sampling configuration.
    fn new(sampling: bool, reservoir_size: usize) -> Self {
        if sampling {
            AtomicHistogram::Sampled(AtomicSamplingReservoir::new(reservoir_size))
        } else {
            AtomicHistogram::Raw(AtomicBucket::new())
        }
    }

    /// Records a new value in the histogram.
    pub fn record(&self, value: f64) {
        match self {
            AtomicHistogram::Raw(bucket) => bucket.push(value),
            AtomicHistogram::Sampled(reservoir) => reservoir.push(value),
        }
    }

    /// Flushes the histogram, calling the given closure with the calculated sample rate and an iterator over the
    /// histogram values.
    ///
    /// Depending on the underlying histogram implementation, the closure may be called multiple times. Callers are
    /// responsible for using the sample rate and reported length of the iterator ([`Values<'a>`] implements
    /// [`ExactSizeIterator`]) to calculate the unsampled length of the histogram.
    pub fn flush<F>(&self, mut f: F)
    where
        F: FnMut(f64, Values<'_>),
    {
        match self {
            AtomicHistogram::Raw(bucket) => bucket.clear_with(|values| {
                f(1.0, Values::Raw(values.iter()));
            }),
            AtomicHistogram::Sampled(reservoir) => reservoir.consume(|values| {
                f(values.sample_rate(), Values::Sampled(values));
            }),
        }
    }
}

impl HistogramFn for AtomicHistogram {
    fn record(&self, value: f64) {
        self.record(value);
    }
}

pub(crate) enum Values<'a> {
    Raw(Iter<'a, f64>),
    Sampled(Drain<'a>),
}

impl<'a> Iterator for Values<'a> {
    type Item = f64;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Values::Raw(values) => values.next().copied(),
            Values::Sampled(drain) => drain.next(),
        }
    }
}

impl<'a> ExactSizeIterator for Values<'a> {
    fn len(&self) -> usize {
        match self {
            Values::Raw(values) => values.len(),
            Values::Sampled(drain) => drain.len(),
        }
    }
}

/// Client-side aggregated metrics storage.
///
/// This storage implementation is designed to be used for aggregating metric values on the client side before sending
/// them to the server. This allows for more efficient transmission of metrics data to the DogStatsD server, as each
/// individual point does not have to be emitted over the network.
///
/// # Behavior
///
/// - Counters are aggregated by summing the increments since the last flush.
/// - Gauges simply maintain their standard "last write wins" behavior and emit the latest value when flushed.
/// - Histograms have their individual values stored as there ia no suitable way to aggregate them.
///
/// # Absolute versus incremental updates to counters
///
/// As we must support both absolute and incremental updates to counters, we need to be able to differentiate between
/// the two cases, which we do in the following way: if a counter is updated absolutely, and the _last_ update was not
/// an absolute value, we reset the counter's state such that the next immediate flush will return a delta of zero.
///
/// This means that a counter needs to have two consecutive absolute updates before it will start emitting deltas, as a
/// stable starting point is required to calculate deltas from. This also means that if a counter has incremental
/// updates that have not yet been flushed, and an absolute update comes in before the next flush, those updates could
/// effectively be lost unless the absolute value accounts for them.
///
/// This should almost never be a concern in practice, as mixing incremental and absolute values is exceedingly rare.
pub(crate) struct ClientSideAggregatedStorage {
    histogram_sampling: bool,
    histogram_reservoir_size: usize,
}

impl ClientSideAggregatedStorage {
    /// Creates a new `ClientSideAggregatedStorage`.
    pub fn new(histogram_sampling: bool, histogram_reservoir_size: usize) -> Self {
        Self { histogram_sampling, histogram_reservoir_size }
    }
}

impl Storage<Key> for ClientSideAggregatedStorage {
    type Counter = Arc<AtomicCounter>;
    type Gauge = Arc<AtomicGauge>;
    type Histogram = Arc<AtomicHistogram>;

    fn counter(&self, _: &Key) -> Self::Counter {
        Arc::new(AtomicCounter::new())
    }

    fn gauge(&self, _: &Key) -> Self::Gauge {
        Arc::new(AtomicGauge::new())
    }

    fn histogram(&self, _: &Key) -> Self::Histogram {
        Arc::new(AtomicHistogram::new(self.histogram_sampling, self.histogram_reservoir_size))
    }
}

#[cfg(test)]
mod tests {
    use metrics::{CounterFn as _, GaugeFn as _};

    use super::{AtomicCounter, AtomicGauge};

    #[test]
    fn atomic_counter_increment() {
        let counter = AtomicCounter::new();
        assert_eq!(counter.flush(), (0, 0));

        counter.increment(42);
        assert_eq!(counter.flush(), (42, 1));

        let large_amount = u64::MAX - u64::from(u16::MAX);
        counter.increment(large_amount);
        assert_eq!(counter.flush(), (large_amount, 1));
    }

    #[test]
    fn atomic_counter_absolute() {
        let first_value = 42;

        let second_value_delta = 87;
        let second_value = first_value + second_value_delta;

        let third_value_delta = 13;
        let third_value = second_value + third_value_delta;

        let counter = AtomicCounter::new();
        assert_eq!(counter.flush(), (0, 0));

        counter.absolute(first_value);
        assert_eq!(counter.flush(), (0, 1));

        counter.absolute(second_value);
        assert_eq!(counter.flush(), (second_value_delta, 1));

        counter.absolute(third_value);
        assert_eq!(counter.flush(), (third_value_delta, 1));
    }

    #[test]
    fn atomic_counter_absolute_multiple_updates_before_first_flush() {
        let first_value = 42;
        let second_value_delta = 66;
        let second_value = first_value + second_value_delta;

        let counter = AtomicCounter::new();
        assert_eq!(counter.flush(), (0, 0));

        counter.absolute(first_value);
        counter.absolute(second_value);
        assert_eq!(counter.flush(), (second_value_delta, 2));
    }

    #[test]
    fn atomic_counter_incremental_to_absolute_reset() {
        let counter = AtomicCounter::new();
        assert_eq!(counter.flush(), (0, 0));

        counter.increment(27);
        counter.absolute(42);
        assert_eq!(counter.flush(), (0, 2));

        counter.increment(13);
        assert_eq!(counter.flush(), (13, 1));

        counter.absolute(87);
        assert_eq!(counter.flush(), (0, 1));

        counter.increment(78);
        assert_eq!(counter.flush(), (78, 1));
    }

    #[test]
    fn atomic_gauge_increment() {
        let gauge = AtomicGauge::new();
        assert_eq!(gauge.flush(), (0.0, 0));

        gauge.increment(42.0);
        assert_eq!(gauge.flush(), (42.0, 1));

        gauge.increment(13.0);
        assert_eq!(gauge.flush(), (55.0, 1));
    }

    #[test]
    fn atomic_gauge_decrement() {
        let gauge = AtomicGauge::new();
        assert_eq!(gauge.flush(), (0.0, 0));

        gauge.decrement(42.0);
        assert_eq!(gauge.flush(), (-42.0, 1));

        gauge.decrement(13.0);
        assert_eq!(gauge.flush(), (-55.0, 1));
    }

    #[test]
    fn atomic_gauge_set() {
        let gauge = AtomicGauge::new();
        assert_eq!(gauge.flush(), (0.0, 0));

        gauge.set(42.0);
        assert_eq!(gauge.flush(), (42.0, 1));

        gauge.set(-13.0);
        assert_eq!(gauge.flush(), (-13.0, 1));
    }
}
