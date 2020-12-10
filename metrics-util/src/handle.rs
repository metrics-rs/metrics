use crate::AtomicBucket;

use atomic_shim::AtomicU64;
use metrics::GaugeValue;
use std::sync::{atomic::Ordering, Arc};

/// Basic metric handle.
///
/// Provides fast, thread-safe access and storage for the three supported metric types: counters,
/// gauges, and histograms.
#[derive(Debug, Clone)]
pub enum Handle {
    /// A counter.
    Counter(Arc<AtomicU64>),

    /// A gauge.
    Gauge(Arc<AtomicU64>),

    /// A histogram.
    Histogram(Arc<AtomicBucket<f64>>),
}

impl Handle {
    /// Creates a counter handle.
    ///
    /// The counter is initialized to 0.
    pub fn counter() -> Handle {
        Handle::Counter(Arc::new(AtomicU64::new(0)))
    }

    /// Creates a gauge handle.
    ///
    /// The gauge is initialized to 0.
    pub fn gauge() -> Handle {
        Handle::Gauge(Arc::new(AtomicU64::new(0)))
    }

    /// Creates a histogram handle.
    ///
    /// The histogram handle is initialized to empty.
    pub fn histogram() -> Handle {
        Handle::Histogram(Arc::new(AtomicBucket::new()))
    }

    /// Increments this handle as a counter.
    ///
    /// Panics if this handle is not a counter.
    pub fn increment_counter(&self, value: u64) {
        match self {
            Handle::Counter(counter) => {
                counter.fetch_add(value, Ordering::SeqCst);
            }
            _ => panic!("tried to increment as counter"),
        }
    }

    /// Updates this handle as a gauge.
    ///
    /// Panics if this handle is not a gauge.
    pub fn update_gauge(&self, value: GaugeValue) {
        match self {
            Handle::Gauge(gauge) => {
                let _ = gauge.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |curr| {
                    let input = f64::from_bits(curr);
                    let output = value.update_value(input);
                    Some(output.to_bits())
                });
            }
            _ => panic!("tried to update as gauge"),
        }
    }

    /// Records to this handle as a histogram.
    ///
    /// Panics if this handle is not a histogram.
    pub fn record_histogram(&self, value: f64) {
        match self {
            Handle::Histogram(bucket) => bucket.push(value),
            _ => panic!("tried to record as histogram"),
        }
    }

    /// Reads this handle as a counter.
    ///
    /// Panics if this handle is not a counter.
    pub fn read_counter(&self) -> u64 {
        match self {
            Handle::Counter(counter) => counter.load(Ordering::Relaxed),
            _ => panic!("tried to read as counter"),
        }
    }

    /// Reads this handle as a gauge.
    ///
    /// Panics if this handle is not a gauge.
    pub fn read_gauge(&self) -> f64 {
        match self {
            Handle::Gauge(gauge) => {
                let unsigned = gauge.load(Ordering::Relaxed);
                f64::from_bits(unsigned)
            }
            _ => panic!("tried to read as gauge"),
        }
    }

    /// Reads this handle as a histogram.
    ///
    /// Panics if this handle is not a histogram.
    pub fn read_histogram(&self) -> Vec<f64> {
        match self {
            Handle::Histogram(bucket) => bucket.data(),
            _ => panic!("tried to read as histogram"),
        }
    }

    /// Reads this handle as a histogram, and whether or not it's empty.
    ///
    /// Panics if this handle is not a histogram.
    pub fn read_histogram_is_empty(&self) -> bool {
        match self {
            Handle::Histogram(bucket) => bucket.is_empty(),
            _ => panic!("tried to read as histogram"),
        }
    }

    /// Reads this handle as a histogram incrementally into a closure, and clears the histogram.
    ///
    /// The closure `f` passed in is invoked multiple times with slices of values present in the
    /// histogram currently.  Once all values have been read, the histogram is cleared of all values.
    ///
    /// Panics if this handle is not a histogram.
    pub fn read_histogram_with_clear<F>(&self, f: F)
    where
        F: FnMut(&[f64]),
    {
        match self {
            Handle::Histogram(bucket) => bucket.clear_with(f),
            _ => panic!("tried to read as histogram"),
        }
    }
}
