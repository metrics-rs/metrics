use std::num::NonZeroU32;
use std::time::Duration;
use std::{collections::HashMap, sync::Arc};

use quanta::Instant;

use crate::common::Matcher;
use crate::native_histogram::{NativeHistogram, NativeHistogramConfig};

use metrics_util::{
    storage::{Histogram, Summary},
    Quantile,
};

const DEFAULT_SUMMARY_BUCKET_COUNT: NonZeroU32 = match NonZeroU32::new(3) {
    Some(v) => v,
    None => unreachable!(),
};
const DEFAULT_SUMMARY_BUCKET_DURATION: Duration = Duration::from_secs(20);

/// Distribution type.
#[derive(Clone, Debug)]
pub enum Distribution {
    /// A Prometheus histogram.
    ///
    /// Exposes "bucketed" values to Prometheus, counting the number of samples
    /// below a given threshold i.e. 100 requests faster than 20ms, 1000 requests
    /// faster than 50ms, etc.
    Histogram(Histogram),
    /// A Prometheus summary.
    ///
    /// Computes and exposes value quantiles directly to Prometheus i.e. 50% of
    /// requests were faster than 200ms, and 99% of requests were faster than
    /// 1000ms, etc.
    Summary(RollingSummary, Arc<Vec<Quantile>>, f64),
    /// A Prometheus native histogram.
    ///
    /// Uses exponential buckets to efficiently represent histogram data without
    /// requiring predefined bucket boundaries.
    NativeHistogram(NativeHistogram),
}

impl Distribution {
    /// Creates a histogram distribution.
    ///
    /// # Panics
    ///
    /// Panics if `buckets` is empty.
    pub fn new_histogram(buckets: &[f64]) -> Distribution {
        let hist = Histogram::new(buckets).expect("buckets should never be empty");
        Distribution::Histogram(hist)
    }

    /// Creates a summary distribution.
    pub fn new_summary(
        quantiles: Arc<Vec<Quantile>>,
        bucket_duration: Duration,
        bucket_count: NonZeroU32,
    ) -> Distribution {
        Distribution::Summary(RollingSummary::new(bucket_count, bucket_duration), quantiles, 0.0)
    }

    /// Creates a native histogram distribution.
    pub fn new_native_histogram(config: NativeHistogramConfig) -> Distribution {
        let hist = NativeHistogram::new(config);
        Distribution::NativeHistogram(hist)
    }

    /// Records the given `samples` in the current distribution.
    ///
    /// Each entry is `(value, count, timestamp)`, recording `value` as if it
    /// had been observed `count` times at `timestamp`. Recording is O(1) in
    /// `count` for every distribution type.
    pub fn record_samples(&mut self, samples: &[(f64, usize, Instant)]) {
        for &(value, count, ts) in samples {
            // A zero count records nothing; skipping it here also keeps an
            // infinite value from poisoning the summary sum (inf * 0.0 == NaN).
            if count == 0 {
                continue;
            }
            match self {
                Distribution::Histogram(hist) => hist.record_n(value, count),
                Distribution::Summary(hist, _, sum) => {
                    hist.add_n(value, count, ts);
                    #[allow(clippy::cast_precision_loss)]
                    {
                        *sum += value * (count as f64);
                    }
                }
                Distribution::NativeHistogram(hist) => hist.observe_n(value, count),
            }
        }
    }
}

/// Builds distributions for metric names based on a set of configured overrides.
#[derive(Debug)]
pub struct DistributionBuilder {
    quantiles: Arc<Vec<Quantile>>,
    buckets: Option<Vec<f64>>,
    bucket_duration: Option<Duration>,
    bucket_count: Option<NonZeroU32>,
    bucket_overrides: Option<Vec<(Matcher, Vec<f64>)>>,
    native_histogram_overrides: Option<Vec<(Matcher, NativeHistogramConfig)>>,
}

impl DistributionBuilder {
    /// Creates a new instance of `DistributionBuilder`.
    pub fn new(
        quantiles: Vec<Quantile>,
        bucket_duration: Option<Duration>,
        buckets: Option<Vec<f64>>,
        bucket_count: Option<NonZeroU32>,
        bucket_overrides: Option<HashMap<Matcher, Vec<f64>>>,
        native_histogram_overrides: Option<HashMap<Matcher, NativeHistogramConfig>>,
    ) -> DistributionBuilder {
        DistributionBuilder {
            quantiles: Arc::new(quantiles),
            bucket_duration,
            buckets,
            bucket_count,
            bucket_overrides: bucket_overrides.map(|entries| {
                let mut matchers = entries.into_iter().collect::<Vec<_>>();
                matchers.sort_by(|a, b| a.0.cmp(&b.0));
                matchers
            }),
            native_histogram_overrides: native_histogram_overrides.map(|entries| {
                let mut matchers = entries.into_iter().collect::<Vec<_>>();
                matchers.sort_by(|a, b| a.0.cmp(&b.0));
                matchers
            }),
        }
    }

    /// Returns a distribution for the given metric key.
    pub fn get_distribution(&self, name: &str) -> Distribution {
        // Check for native histogram overrides first (highest priority)
        if let Some(ref overrides) = self.native_histogram_overrides {
            for (matcher, config) in overrides {
                if matcher.matches(name) {
                    return Distribution::new_native_histogram(config.clone());
                }
            }
        }

        // Check for histogram bucket overrides
        if let Some(ref overrides) = self.bucket_overrides {
            for (matcher, buckets) in overrides {
                if matcher.matches(name) {
                    return Distribution::new_histogram(buckets);
                }
            }
        }

        // Check for global histogram buckets
        if let Some(ref buckets) = self.buckets {
            return Distribution::new_histogram(buckets);
        }

        // Default to summary
        let b_duration = self.bucket_duration.map_or(DEFAULT_SUMMARY_BUCKET_DURATION, |d| d);
        let b_count = self.bucket_count.map_or(DEFAULT_SUMMARY_BUCKET_COUNT, |c| c);

        Distribution::new_summary(self.quantiles.clone(), b_duration, b_count)
    }

    /// Returns the distribution type for the given metric key.
    pub fn get_distribution_type(&self, name: &str) -> &'static str {
        // Check for native histogram overrides first (highest priority)
        if let Some(ref overrides) = self.native_histogram_overrides {
            for (matcher, _) in overrides {
                if matcher.matches(name) {
                    return "native_histogram";
                }
            }
        }

        // Check for regular histogram buckets
        if self.buckets.is_some() {
            return "histogram";
        }

        if let Some(ref overrides) = self.bucket_overrides {
            for (matcher, _) in overrides {
                if matcher.matches(name) {
                    return "histogram";
                }
            }
        }

        "summary"
    }
}

#[derive(Clone, Debug)]
struct Bucket {
    begin: Instant,
    summary: Summary,
}

/// A `RollingSummary` manages a list of [Summary] so that old results can be expired.
#[derive(Clone, Debug)]
pub struct RollingSummary {
    // Buckets are ordered with the latest buckets first.  The buckets are kept in alignment based
    // on the instant of the first added bucket and the bucket_duration.  There may be gaps in the
    // bucket list.
    buckets: Vec<Bucket>,
    // Maximum number of buckets to track.
    max_buckets: usize,
    // Duration of values stored per bucket.
    bucket_duration: Duration,
    // This is the maximum duration a bucket will be kept.
    max_bucket_duration: Duration,
    // Total samples since creation of this summary.  This is separate from the Summary since it is
    // never reset. u64 rather than usize: weighted counts make 2^32 reachable
    // on 32-bit targets, and a wrapped total renders as a counter reset.
    count: u64,
}

impl Default for RollingSummary {
    fn default() -> Self {
        RollingSummary::new(DEFAULT_SUMMARY_BUCKET_COUNT, DEFAULT_SUMMARY_BUCKET_DURATION)
    }
}

impl RollingSummary {
    /// Create a new `RollingSummary` with the given number of `buckets` and `bucket-duration`.
    ///
    /// The summary will store quantiles over `buckets * bucket_duration` seconds.
    pub fn new(buckets: std::num::NonZeroU32, bucket_duration: Duration) -> RollingSummary {
        assert!(!bucket_duration.is_zero());
        let max_bucket_duration = bucket_duration * buckets.get();
        let max_buckets = buckets.get() as usize;

        RollingSummary {
            buckets: Vec::with_capacity(max_buckets),
            max_buckets,
            bucket_duration,
            max_bucket_duration,
            count: 0,
        }
    }

    /// Add a sample `value` to the `RollingSummary` at the time `now`.
    ///
    /// Any values that expire at the `value_ts` are removed from the `RollingSummary`.
    pub fn add(&mut self, value: f64, now: Instant) {
        self.add_n(value, 1, now);
    }

    /// Add a sample `value` to the `RollingSummary` `n` times, as if `add` had been called `n`
    /// times with the same `now`, in constant time.
    ///
    /// Any values that expire at the `value_ts` are removed from the `RollingSummary`.
    pub fn add_n(&mut self, value: f64, n: usize, now: Instant) {
        if n == 0 {
            return;
        }

        // The count is incremented even if this value is too old to be saved in any bucket.
        self.count += n as u64;

        // If we can find a bucket that this value belongs in, then we can just add it in and be
        // done.
        for bucket in &mut self.buckets {
            let end = bucket.begin + self.bucket_duration;

            // If this value belongs in a future bucket...
            if now > bucket.begin + self.bucket_duration {
                break;
            }

            if now >= bucket.begin && now < end {
                bucket.summary.add_n(value, n);
                return;
            }
        }

        // Remove any expired buckets.
        if let Some(cutoff) = now.checked_sub(self.max_bucket_duration) {
            self.buckets.retain(|b| b.begin > cutoff);
        }

        if self.buckets.is_empty() {
            let mut summary = Summary::with_defaults();
            summary.add_n(value, n);
            self.buckets.push(Bucket { begin: now, summary });
            return;
        }

        // Take the first bucket time as a reference.  Other buckets will be created at an offset
        // of this time.  We know this time is close to the value_ts, if it were much older the
        // bucket would have been removed.
        let reftime = self.buckets[0].begin;

        let mut summary = Summary::with_defaults();
        summary.add_n(value, n);

        // If the value is newer than the first bucket then count upwards to the new bucket time.
        let mut begin;
        if now > reftime {
            begin = reftime + self.bucket_duration;
            let mut end = begin + self.bucket_duration;
            while now < begin || now >= end {
                begin += self.bucket_duration;
                end += self.bucket_duration;
            }

            self.buckets.truncate(self.max_buckets - 1);
            self.buckets.insert(0, Bucket { begin, summary });
        }
    }

    /// Return a merged Summary of all items that are valid at `now`.
    ///
    /// # Warning
    ///
    /// The snapshot `Summary::count()` contains the total number of values considered in the
    /// Snapshot, which is not the full count of the `RollingSummary`.  Use `RollingSummary::count()`
    /// instead.
    pub fn snapshot(&self, now: Instant) -> Summary {
        let cutoff = now.checked_sub(self.max_bucket_duration);
        let mut acc = Summary::with_defaults();
        self.buckets
            .iter()
            .filter(|b| if let Some(cutoff) = cutoff { b.begin > cutoff } else { true })
            .map(|b| &b.summary)
            .fold(&mut acc, |acc, item| {
                acc.merge(item).expect("merge can only fail if summary config inconsistent");
                acc
            });
        acc
    }

    /// Whether or not this summary is empty.
    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }

    /// Gets the totoal number of samples this summary has seen so far.
    pub fn count(&self) -> u64 {
        self.count
    }

    #[cfg(test)]
    fn buckets(&self) -> &Vec<Bucket> {
        &self.buckets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use quanta::Clock;

    #[test]
    #[allow(clippy::float_cmp)]
    fn record_samples_ignores_zero_count_entries() {
        // Zero-count entries can only arrive via direct callers of this pub
        // API (the registry filters them); an infinite value with count zero
        // must not poison the summary sum — the one piece of state updated
        // here rather than in a callee with its own zero guard. The weighted
        // happy path is covered end to end in tests/record_many.rs.
        let (clock, mock) = Clock::mock();
        mock.increment(Duration::from_secs(3600));
        let now = clock.now();

        let mut dist = Distribution::new_summary(
            Arc::new(vec![]),
            DEFAULT_SUMMARY_BUCKET_DURATION,
            DEFAULT_SUMMARY_BUCKET_COUNT,
        );
        dist.record_samples(&[(2.0, 3, now), (f64::INFINITY, 0, now)]);
        let Distribution::Summary(summary, _, sum) = &dist else {
            panic!("expected summary");
        };
        assert_eq!(summary.count(), 3);
        assert_eq!(*sum, 6.0);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn rolling_summary_add_n_equals_repeated_add() {
        let (clock, mock) = Clock::mock();
        mock.increment(Duration::from_secs(3600));

        let mut repeated = RollingSummary::default();
        let mut counted = RollingSummary::default();

        for (v, n) in [(42.0, 500usize), (7.0, 250)] {
            for _ in 0..n {
                repeated.add(v, clock.now());
            }
            counted.add_n(v, n, clock.now());
            mock.increment(Duration::from_secs(20));
        }
        counted.add_n(9.0, 0, clock.now()); // no-op

        assert_eq!(repeated.count(), counted.count());
        assert_eq!(repeated.buckets().len(), counted.buckets().len());
        let now = clock.now();
        let (r, c) = (repeated.snapshot(now), counted.snapshot(now));
        assert_eq!(r.count(), c.count());
        for q in [0.25, 0.5, 0.75, 0.99] {
            assert_eq!(r.quantile(q), c.quantile(q), "quantile {q} diverged");
        }
    }

    #[test]
    fn new_rolling_summary() {
        let summary = RollingSummary::default();

        assert_eq!(0, summary.buckets().len());
        assert_eq!(0, summary.count());
        assert!(summary.is_empty());
    }

    #[test]
    fn empty_snapshot() {
        let (clock, _mock) = Clock::mock();
        let summary = RollingSummary::default();
        let snapshot = summary.snapshot(clock.now());

        assert_eq!(0, snapshot.count());
        #[allow(clippy::float_cmp)]
        {
            assert_eq!(f64::INFINITY, snapshot.min());
            assert_eq!(f64::NEG_INFINITY, snapshot.max());
        }
        assert_eq!(None, snapshot.quantile(0.5));
    }

    #[test]
    fn snapshot() {
        let (clock, mock) = Clock::mock();
        mock.increment(Duration::from_secs(3600));

        let mut summary = RollingSummary::default();
        summary.add(42.0, clock.now());
        mock.increment(Duration::from_secs(20));
        summary.add(42.0, clock.now());
        mock.increment(Duration::from_secs(20));
        summary.add(42.0, clock.now());

        let snapshot = summary.snapshot(clock.now());

        #[allow(clippy::float_cmp)]
        {
            assert_eq!(42.0, snapshot.min());
            assert_eq!(42.0, snapshot.max());
        }
        // 42 +/- (42 * 0.0001)
        assert!(Some(41.9958) < snapshot.quantile(0.5));
        assert!(Some(42.0042) > snapshot.quantile(0.5));
    }

    #[test]
    fn add_first_value() {
        let (clock, mock) = Clock::mock();
        mock.increment(Duration::from_secs(3600));

        let mut summary = RollingSummary::default();
        summary.add(42.0, clock.now());

        assert_eq!(1, summary.buckets().len());
        assert_eq!(1, summary.count());
        assert!(!summary.is_empty());
    }

    #[test]
    fn add_new_head() {
        let (clock, mock) = Clock::mock();
        mock.increment(Duration::from_secs(3600));

        let mut summary = RollingSummary::default();
        summary.add(42.0, clock.now());
        mock.increment(Duration::from_secs(20));
        summary.add(42.0, clock.now());

        assert_eq!(2, summary.buckets().len());
    }

    #[test]
    fn truncate_old_buckets() {
        let (clock, mock) = Clock::mock();
        mock.increment(Duration::from_secs(3600));

        let mut summary = RollingSummary::default();
        summary.add(42.0, clock.now());

        for _ in 0..3 {
            mock.increment(Duration::from_secs(20));
            summary.add(42.0, clock.now());
        }

        assert_eq!(3, summary.buckets().len());
    }

    #[test]
    fn add_value_ts_before_first_bucket() {
        let (clock, mock) = Clock::mock();
        mock.increment(Duration::from_secs(4));

        let bucket_count = NonZeroU32::new(2).unwrap();
        let bucket_width = Duration::from_secs(5);

        let mut summary = RollingSummary::new(bucket_count, bucket_width);
        assert_eq!(0, summary.buckets().len());
        assert_eq!(0, summary.count());

        // Add a single value to create our first bucket.
        summary.add(42.0, clock.now());

        // Make sure the value got added.
        assert_eq!(1, summary.buckets().len());
        assert_eq!(1, summary.count());
        assert!(!summary.is_empty());

        // Our first bucket is now marked as begin=4/width=5, so make sure that if we add a version
        // with now=3, the count goes up but it's not actually added.
        mock.decrement(Duration::from_secs(1));

        summary.add(43.0, clock.now());

        assert_eq!(1, summary.buckets().len());
        assert_eq!(2, summary.count());
        assert!(!summary.is_empty());
    }
}
