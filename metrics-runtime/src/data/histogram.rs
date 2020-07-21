use crate::common::{Delta, ValueHandle};
use crate::helper::duration_as_nanos;
use atomic_shim::AtomicU64;
use crossbeam_utils::Backoff;
use metrics_util::{AtomicBucket, StreamingIntegers};
use quanta::Clock;
use std::cmp;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// A reference to a [`Histogram`].
///
/// A [`Histogram`] is used for directly updating a gauge, without any lookup overhead.
#[derive(Clone)]
pub struct Histogram {
    handle: ValueHandle,
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

impl From<ValueHandle> for Histogram {
    fn from(handle: ValueHandle) -> Self {
        Self { handle }
    }
}

/// An atomic windowed histogram.
///
/// This histogram provides a windowed view of values that rolls forward over time, dropping old
/// values as they exceed the window of the histogram.  Writes into the histogram are lock-free, as
/// well as snapshots of the histogram.
#[derive(Debug)]
pub struct AtomicWindowedHistogram {
    buckets: Vec<AtomicBucket<u64>>,
    bucket_count: usize,
    granularity: u64,
    upkeep_index: AtomicUsize,
    index: AtomicUsize,
    next_upkeep: AtomicU64,
    clock: Clock,
}

impl AtomicWindowedHistogram {
    /// Creates a new [`AtomicWindowedHistogram`].
    ///
    /// Internally, a number of buckets will be created, based on how many times `granularity` goes
    /// into `window`.  As time passes, buckets will be cleared to avoid values older than the
    /// `window` duration.
    ///
    /// As buckets will hold values represneting a period of time up to `granularity`, the
    /// granularity can be lowered or raised to roll values off more precisely, or less precisely,
    /// against the provided clock.
    ///
    /// # Panics
    /// Panics if `granularity` is larger than `window`.
    pub fn new(window: Duration, granularity: Duration, clock: Clock) -> Self {
        let window_ns = duration_as_nanos(window);
        let granularity_ns = duration_as_nanos(granularity);
        assert!(window_ns > granularity_ns);
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
            upkeep_index: AtomicUsize::new(0),
            index: AtomicUsize::new(0),
            next_upkeep: AtomicU64::new(next_upkeep),
            clock,
        }
    }

    /// Takes a snapshot of the current histogram.
    ///
    /// Returns a [`StreamingIntegers`] value, representing all observed values in the
    /// histogram.  As writes happen concurrently, along with buckets being cleared, a snapshot is
    /// not guaranteed to have all values present at the time the method was called.
    pub fn snapshot(&self) -> StreamingIntegers {
        // Run upkeep to make sure our window reflects any time passage since the last write.
        let index = self.upkeep();

        let mut streaming = StreamingIntegers::new();

        // Start from the bucket ahead of the currently-being-written-to-bucket so that we outrace
        // any upkeep and get access to more of the data.
        for i in 0..self.bucket_count {
            let bucket_index = (index + i + 1) % self.bucket_count;
            let bucket = &self.buckets[bucket_index];
            bucket.data_with(|block| streaming.compress(block));
        }
        streaming
    }

    /// Records a value to the histogram.
    pub fn record(&self, value: u64) {
        let index = self.upkeep();
        self.buckets[index].push(value);
    }

    fn upkeep(&self) -> usize {
        let backoff = Backoff::new();

        loop {
            // Start by figuring out if the histogram needs to perform upkeep.
            let now = self.clock.recent();
            let next_upkeep = self.next_upkeep.load(Ordering::Acquire);
            if now <= next_upkeep {
                let index = self.index.load(Ordering::Acquire);
                let actual_index = index % self.bucket_count;

                return actual_index;
            }

            // We do need to perform upkeep, but someone *else* might actually be doing it already,
            // so go ahead and wait until the index is caught up with the upkeep index: the upkeep
            // index will be ahead of index until upkeep is complete.
            let mut upkeep_in_progress = false;
            let mut index;
            loop {
                index = self.index.load(Ordering::Acquire);
                let upkeep_index = self.upkeep_index.load(Ordering::Acquire);
                if index == upkeep_index {
                    break;
                }

                upkeep_in_progress = true;
                backoff.snooze();
            }

            // If we waited for another upkeep operation to complete, then there's the chance that
            // enough time has passed that we're due for upkeep again, so restart our loop.
            if upkeep_in_progress {
                continue;
            }

            // Figure out how many buckets, up to the maximum, need to be cleared based on the
            // delta between the target upkeep time and the actual time.  We always clear at least
            // one bucket, but may need to clear them all.
            let delta = now - next_upkeep;
            let bucket_depth = cmp::min((delta / self.granularity) as usize, self.bucket_count) + 1;

            // Now that we we know how many buckets we need to clear, update the index to pointer
            // writers at the next bucket past the last one that we will be clearing.
            let new_index = index + bucket_depth;
            let prev_index = self
                .index
                .compare_and_swap(index, new_index, Ordering::SeqCst);
            if prev_index == index {
                // Clear the target bucket first, and then update the upkeep target time so new
                // writers can proceed.  We may still have other buckets to clean up if we had
                // multiple rounds worth of upkeep to do, but this will let new writes proceed as
                // soon as possible.
                let clear_index = new_index % self.bucket_count;
                self.buckets[clear_index].clear();

                let now = self.clock.now();
                let next_upkeep = now + self.granularity;
                self.next_upkeep.store(next_upkeep, Ordering::Release);

                // Now that we've cleared the actual bucket that writers will use going forward, we
                // have to clear any older buckets that we skipped over.  If our granularity was 1
                // second, and we skipped over 4 seconds worth of buckets, we would still have
                // 3 buckets to clear, etc.
                let last_index = new_index - 1;
                while index < last_index {
                    index += 1;
                    let clear_index = index % self.bucket_count;
                    self.buckets[clear_index].clear();
                }

                // We've cleared the old buckets, so upkeep is done.  Push our upkeep index forward
                // so that writers who were blocked waiting for upkeep to conclude can restart.
                self.upkeep_index.store(new_index, Ordering::Release);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AtomicWindowedHistogram, Clock};
    use crossbeam_utils::thread;
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

        // Set our granularity at right below a second, so that when we when add a second, we don't
        // land on the same exact value, and our "now" time should always be ahead of the upkeep
        // time when we expect it to be.
        let h =
            AtomicWindowedHistogram::new(Duration::from_secs(5), Duration::from_millis(999), clock);

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

        // We should also be able to advance by vast periods of time and observe not only old
        // values going away but no weird overflow issues or index or anything.  This ensures that
        // our upkeep code functions not just for under-load single bucket rollovers but also "been
        // idle for a while and just got a write" scenarios.
        h.record(42);

        let snapshot = h.snapshot();
        assert_eq!(snapshot.len(), 1);
        let total: u64 = snapshot.decompress().iter().sum();
        assert_eq!(total, 42);

        ctl.increment(Duration::from_secs(1000));
        let snapshot = h.snapshot();
        assert_eq!(snapshot.len(), 0);
    }

    #[test]
    fn test_histogram_write_gauntlet_mt() {
        let clock = Clock::new();
        let clock2 = clock.clone();
        let target = clock.now() + Duration::from_secs(5).as_nanos() as u64;
        let h = AtomicWindowedHistogram::new(
            Duration::from_secs(20),
            Duration::from_millis(500),
            clock,
        );

        thread::scope(|s| {
            let t1 = s.spawn(|_| {
                let mut total = 0;
                while clock2.now() < target {
                    h.record(42);
                    total += 1;
                }
                total
            });
            let t2 = s.spawn(|_| {
                let mut total = 0;
                while clock2.now() < target {
                    h.record(42);
                    total += 1;
                }
                total
            });
            let t3 = s.spawn(|_| {
                let mut total = 0;
                while clock2.now() < target {
                    h.record(42);
                    total += 1;
                }
                total
            });

            let t1_total = t1.join().expect("thread 1 panicked during test");
            let t2_total = t2.join().expect("thread 2 panicked during test");
            let t3_total = t3.join().expect("thread 3 panicked during test");

            let total = t1_total + t2_total + t3_total;
            let snap = h.snapshot();
            assert_eq!(total, snap.len());
        })
        .unwrap();
    }
}
