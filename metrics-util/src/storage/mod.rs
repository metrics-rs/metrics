//! Various data structures for storing metric data.

mod bucket;
use metrics::HistogramFn;

pub use self::bucket::AtomicBucket;

mod histogram;
pub use self::histogram::Histogram;

mod reservoir;
pub use self::reservoir::AtomicSamplingReservoir;

mod summary;
pub use self::summary::Summary;

impl HistogramFn for AtomicBucket<f64> {
    fn record(&self, value: f64) {
        self.push(value);
    }
}

impl HistogramFn for AtomicSamplingReservoir {
    fn record(&self, value: f64) {
        self.push(value);
    }
}
