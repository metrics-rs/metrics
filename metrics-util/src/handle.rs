use std::sync::atomic::{AtomicU64, AtomicI64};
use crate::AtomicBucket;

/// Basic metric handle.
///
/// Provides fast, thread-safe access and storage for the three supported metric types: counters,
/// gauges, and histograms.
pub enum Handle {
    /// A counter.
    Counter(AtomicU64),

    /// A gauge.
    Gauge(AtomicI64),

    /// A histogram.
    Histogram(AtomicBucket<f64>),
}

impl Handle {
    /// Creates a counter handle.
    ///
    /// The counter is initialized to 0.
    pub const fn counter() -> Handle {
        Handle::Counter(AtomicU64::new(0))
    }

    /// Creates a gauge handle.
    ///
    /// The gauge is initialized to 0.
    pub const fn gauge() -> Handle {
        Handle::Gauge(AtomicI64::new(0))
    }

    /// Creates a histogram handle.
    ///
    /// The histogram handle is initialized to empty.
    pub const fn histogram() -> Handle {
        Handle::Histogram(AtomicBucket::new())
    }
}
