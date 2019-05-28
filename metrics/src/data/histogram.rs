use crate::common::{Delta, MetricValue};
use crate::helper::duration_as_nanos;
use metrics_util::{AtomicBucket, StreamingIntegers};
use quanta::Clock;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

/// Proxy object to update a histogram.
pub struct Histogram {
    handle: MetricValue,
}

impl Histogram {
    /// Records a timing for the histogram.
    pub fn record_timing<D: Delta>(&self, start: D, end: D) {
        let value = end.delta(start);
        self.handle.update_histogram(value);
    }

    /// Records a value for the histogram.
    pub fn record_value(&self, value: u64) {
        self.handle.update_histogram(value);
    }
}

impl From<MetricValue> for Histogram {
    fn from(handle: MetricValue) -> Self {
        Self { handle }
    }
}

#[derive(Debug)]
pub struct AtomicWindowedHistogram {
    buckets: Vec<AtomicBucket<u64>>,
    bucket_count: usize,
    granularity: u64,
    index: AtomicUsize,
    next_upkeep: AtomicU64,
    clock: Clock,
}

impl AtomicWindowedHistogram {
    pub fn new(window: Duration, granularity: Duration, clock: Clock) -> Self {
        let window_ns = duration_as_nanos(window);
        let granularity_ns = duration_as_nanos(granularity);
        let now = clock.recent();

        let bucket_count = ((window_ns / granularity_ns) as usize) + 1;
        let mut buckets = Vec::new();
        for _ in 0..bucket_count {
            buckets.push(AtomicBucket::new());
        }

        let next_upkeep = now + granularity_ns;

        AtomicWindowedHistogram {
            buckets,
            bucket_count,
            granularity: granularity_ns,
            index: AtomicUsize::new(0),
            next_upkeep: AtomicU64::new(next_upkeep),
            clock,
        }
    }

    pub fn snapshot(&self) -> StreamingIntegers {
        // Run upkeep to make sure our window reflects any time passage since the last write.
        let _ = self.upkeep();

        let mut streaming = StreamingIntegers::new();
        for bucket in &self.buckets {
            bucket.data_with(|block| streaming.compress(block));
        }
        streaming
    }

    pub fn record(&self, value: u64) {
        let index = self.upkeep();
        self.buckets[index].push(value);
    }

    fn upkeep(&self) -> usize {
        loop {
            let now = self.clock.recent();
            let index = self.index.load(Ordering::Acquire);

            // See if we need to update the index because we're past our upkeep target.
            let next_upkeep = self.next_upkeep.load(Ordering::Acquire);
            if now < next_upkeep {
                return index;
            }

            let new_index = (index + 1) % self.bucket_count;
            if self
                .index
                .compare_and_swap(index, new_index, Ordering::AcqRel)
                == index
            {
                // If we've had to update the index, go ahead and clear the bucket in front of our
                // new bucket.  Since we write low to high/left to right, the "oldest" bucket,
                // which is the one that should be dropped, is the one we just updated our index
                // to, but we always add an extra bucket on top of what we need so that we can
                // clear that one, instead of clearing the one we're going to be writing to next so
                // that we don't clear the values of writers who start writing to the new bucket
                // while we're doing the clear.
                self.buckets[new_index].clear();

                // Since another write could outrun us, just do a single CAS.  99.99999999% of the
                // time, the CAS will go through, because it takes nanoseconds and our granularity
                // will be in the hundreds of milliseconds, if not seconds.
                self.next_upkeep.compare_and_swap(
                    next_upkeep,
                    next_upkeep + self.granularity,
                    Ordering::AcqRel,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AtomicWindowedHistogram, Clock};
    use std::time::Duration;

    #[test]
    fn test_histogram_simple_update() {
        let (clock, _ctl) = Clock::mock();
        let h = AtomicWindowedHistogram::new(Duration::from_secs(5), Duration::from_secs(1), clock);

        h.record(1245);

        let snapshot = h.snapshot();
        assert_eq!(snapshot.len(), 1);

        let values = snapshot.decompress();
        assert_eq!(values.len(), 1);
        assert_eq!(values.get(0).unwrap(), &1245);
    }

    #[test]
    fn test_histogram_complex_update() {
        let (clock, _ctl) = Clock::mock();
        let h = AtomicWindowedHistogram::new(Duration::from_secs(5), Duration::from_secs(1), clock);

        h.record(1245);
        h.record(213);
        h.record(1022);
        h.record(1248);

        let snapshot = h.snapshot();
        assert_eq!(snapshot.len(), 4);

        let values = snapshot.decompress();
        assert_eq!(values.len(), 4);
        assert_eq!(values.get(0).unwrap(), &1245);
        assert_eq!(values.get(1).unwrap(), &213);
        assert_eq!(values.get(2).unwrap(), &1022);
        assert_eq!(values.get(3).unwrap(), &1248);
    }

    #[test]
    fn test_windowed_histogram_rollover() {
        let (clock, ctl) = Clock::mock();
        let h = AtomicWindowedHistogram::new(Duration::from_secs(5), Duration::from_secs(1), clock);

        // Histogram is empty, snapshot is empty.
        let snapshot = h.snapshot();
        assert_eq!(snapshot.len(), 0);

        // Immediately add two values, and observe the histogram and snapshot having two values.
        h.record(1);
        h.record(2);
        let snapshot = h.snapshot();
        assert_eq!(snapshot.len(), 2);
        let total: u64 = snapshot.decompress().iter().sum();
        assert_eq!(total, 3);

        // Roll forward 3 seconds, should still have everything.
        ctl.increment(Duration::from_secs(3));
        let snapshot = h.snapshot();
        assert_eq!(snapshot.len(), 2);
        let total: u64 = snapshot.decompress().iter().sum();
        assert_eq!(total, 3);

        // Roll forward 1 second, should still have everything.
        ctl.increment(Duration::from_secs(1));
        let snapshot = h.snapshot();
        assert_eq!(snapshot.len(), 2);
        let total: u64 = snapshot.decompress().iter().sum();
        assert_eq!(total, 3);

        // Roll forward 1 second, should still have everything.
        ctl.increment(Duration::from_secs(1));
        let snapshot = h.snapshot();
        assert_eq!(snapshot.len(), 2);
        let total: u64 = snapshot.decompress().iter().sum();
        assert_eq!(total, 3);

        // Pump in some new values.  We should have a total of 5 values now.
        h.record(3);
        h.record(4);
        h.record(5);

        let snapshot = h.snapshot();
        assert_eq!(snapshot.len(), 5);
        let total: u64 = snapshot.decompress().iter().sum();
        assert_eq!(total, 15);

        // Roll forward 6 seconds, in increments.  The first one rolls over a single bucket, and
        // cleans bucket #0, the first one we wrote to.  The second and third ones get us right up
        // to the last three values, and then clear them out.
        ctl.increment(Duration::from_secs(1));
        let snapshot = h.snapshot();
        assert_eq!(snapshot.len(), 3);
        let total: u64 = snapshot.decompress().iter().sum();
        assert_eq!(total, 12);

        ctl.increment(Duration::from_secs(4));
        let snapshot = h.snapshot();
        assert_eq!(snapshot.len(), 3);
        let total: u64 = snapshot.decompress().iter().sum();
        assert_eq!(total, 12);

        ctl.increment(Duration::from_secs(1));
        let snapshot = h.snapshot();
        assert_eq!(snapshot.len(), 0);
    }
}
