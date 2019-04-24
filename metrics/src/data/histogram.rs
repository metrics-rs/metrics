use crate::{data::ScopedKey, helper::duration_as_nanos};
use fnv::FnvBuildHasher;
use hashbrown::HashMap;
use std::time::{Duration, Instant};

pub(crate) struct Histogram {
    window: Duration,
    granularity: Duration,
    data: HashMap<ScopedKey, WindowedRawHistogram, FnvBuildHasher>,
}

impl Histogram {
    pub fn new(window: Duration, granularity: Duration) -> Histogram {
        Histogram {
            window,
            granularity,
            data: HashMap::default(),
        }
    }

    pub fn update(&mut self, key: ScopedKey, value: u64) {
        if let Some(wh) = self.data.get_mut(&key) {
            wh.update(value);
        } else {
            let mut wh = WindowedRawHistogram::new(self.window, self.granularity);
            wh.update(value);
            let _ = self.data.insert(key, wh);
        }
    }

    pub fn upkeep(&mut self, at: Instant) {
        for (_, histogram) in self.data.iter_mut() {
            histogram.upkeep(at);
        }
    }

    pub fn values(&self) -> Vec<(ScopedKey, HistogramSnapshot)> {
        self.data
            .iter()
            .map(|(k, v)| (k.clone(), v.snapshot()))
            .collect()
    }
}

pub(crate) struct WindowedRawHistogram {
    buckets: Vec<Vec<u64>>,
    num_buckets: usize,
    bucket_index: usize,
    last_upkeep: Instant,
    granularity: Duration,
}

impl WindowedRawHistogram {
    pub fn new(window: Duration, granularity: Duration) -> WindowedRawHistogram {
        let num_buckets =
            ((duration_as_nanos(window) / duration_as_nanos(granularity)) as usize) + 1;
        let mut buckets = Vec::with_capacity(num_buckets);

        for _ in 0..num_buckets {
            let histogram = Vec::new();
            buckets.push(histogram);
        }

        WindowedRawHistogram {
            buckets,
            num_buckets,
            bucket_index: 0,
            last_upkeep: Instant::now(),
            granularity,
        }
    }

    pub fn upkeep(&mut self, at: Instant) {
        if at >= self.last_upkeep + self.granularity {
            self.bucket_index += 1;
            self.bucket_index %= self.num_buckets;
            self.buckets[self.bucket_index].clear();
            self.last_upkeep = at;
        }
    }

    pub fn update(&mut self, value: u64) {
        self.buckets[self.bucket_index].push(value);
    }

    pub fn snapshot(&self) -> HistogramSnapshot {
        let mut aggregate = Vec::new();
        for bucket in &self.buckets {
            aggregate.extend_from_slice(&bucket);
        }

        HistogramSnapshot::new(aggregate)
    }
}

/// A point-in-time snapshot of a single histogram.
#[derive(Debug, PartialEq, Eq)]
pub struct HistogramSnapshot {
    values: Vec<u64>,
}

impl HistogramSnapshot {
    pub(crate) fn new(values: Vec<u64>) -> Self {
        HistogramSnapshot { values }
    }

    /// Gets the raw values that compromise the entire histogram.
    pub fn values(&self) -> &Vec<u64> {
        &self.values
    }
}

#[cfg(test)]
mod tests {
    use super::{Histogram, ScopedKey, WindowedRawHistogram};
    use std::time::{Duration, Instant};

    #[test]
    fn test_histogram_simple_update() {
        let mut histogram = Histogram::new(Duration::new(5, 0), Duration::new(1, 0));

        let key = ScopedKey(0, "foo".into());
        histogram.update(key, 1245);

        let values = histogram.values();
        assert_eq!(values.len(), 1);

        let hdr = &values[0].1;
        assert_eq!(hdr.values().len(), 1);
        assert_eq!(hdr.values().get(0).unwrap(), &1245);
    }

    #[test]
    fn test_histogram_complex_update() {
        let mut histogram = Histogram::new(Duration::new(5, 0), Duration::new(1, 0));

        let key = ScopedKey(0, "foo".into());
        histogram.update(key.clone(), 1245);
        histogram.update(key.clone(), 213);
        histogram.update(key.clone(), 1022);
        histogram.update(key, 1248);

        let values = histogram.values();
        assert_eq!(values.len(), 1);

        let hdr = &values[0].1;
        assert_eq!(hdr.values().len(), 4);
        assert_eq!(hdr.values().get(0).unwrap(), &1245);
        assert_eq!(hdr.values().get(1).unwrap(), &213);
        assert_eq!(hdr.values().get(2).unwrap(), &1022);
        assert_eq!(hdr.values().get(3).unwrap(), &1248);
    }

    #[test]
    fn test_windowed_histogram_rollover() {
        let mut wh = WindowedRawHistogram::new(Duration::new(5, 0), Duration::new(1, 0));
        let now = Instant::now();

        let snapshot = wh.snapshot();
        assert_eq!(snapshot.values().len(), 0);

        wh.update(1);
        wh.update(2);
        let snapshot = wh.snapshot();
        assert_eq!(snapshot.values().len(), 2);

        // Roll forward 3 seconds, should still have everything.
        let now = now + Duration::new(1, 0);
        wh.upkeep(now);
        let snapshot = wh.snapshot();
        assert_eq!(snapshot.values().len(), 2);

        let now = now + Duration::new(1, 0);
        wh.upkeep(now);
        let snapshot = wh.snapshot();
        assert_eq!(snapshot.values().len(), 2);

        let now = now + Duration::new(1, 0);
        wh.upkeep(now);
        let snapshot = wh.snapshot();
        assert_eq!(snapshot.values().len(), 2);

        // Pump in some new values.
        wh.update(3);
        wh.update(4);
        wh.update(5);

        let snapshot = wh.snapshot();
        assert_eq!(snapshot.values().len(), 5);

        // Roll forward 3 seconds, and make sure the first two values are gone.
        // You might think this should be 2 seconds, but we have one extra bucket
        // allocated so that there's always a clear bucket that we can write into.
        // This means we have more than our total window, but only having the exact
        // number of buckets would mean we were constantly missing a bucket's worth
        // of granularity.
        let now = now + Duration::new(1, 0);
        wh.upkeep(now);
        let snapshot = wh.snapshot();
        assert_eq!(snapshot.values().len(), 5);

        let now = now + Duration::new(1, 0);
        wh.upkeep(now);
        let snapshot = wh.snapshot();
        assert_eq!(snapshot.values().len(), 5);

        let now = now + Duration::new(1, 0);
        wh.upkeep(now);
        let snapshot = wh.snapshot();
        assert_eq!(snapshot.values().len(), 3);
    }
}
