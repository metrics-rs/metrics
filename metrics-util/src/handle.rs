use crate::AtomicBucket;

use atomic_shim::AtomicU64;
use metrics::{Counter, Gauge, Histogram, HistogramFn};
use std::sync::Arc;

#[derive(Clone, Default)]
struct AtomicCounter {
    inner: Arc<AtomicU64>,
}

impl AtomicCounter {
    pub fn as_counter(&self) -> Counter {
        let arc = Arc::clone(&self.inner);
        Counter::from_arc(arc)
    }
}

#[derive(Clone, Default)]
struct AtomicGauge {
    inner: Arc<AtomicU64>,
}

impl AtomicGauge {
    pub fn as_gauge(&self) -> Gauge {
        let arc = Arc::clone(&self.inner);
        Gauge::from_arc(arc)
    }
}

#[derive(Clone, Default)]
struct AtomicHistogram {
    inner: Arc<AtomicBucket<f64>>,
}

impl AtomicHistogram {
    pub fn as_histogram(&self) -> Histogram {
        let arc = Arc::clone(&self.inner);
        Histogram::from_arc(arc)
    }
}

impl HistogramFn for AtomicBucket<f64> {
    fn record(&self, value: f64) {
        self.push(value);
    }
}