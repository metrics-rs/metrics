use crate::AtomicBucket;
use metrics::HistogramFn;

impl HistogramFn for AtomicBucket<f64> {
    fn record(&self, value: f64) {
        self.push(value);
    }
}
