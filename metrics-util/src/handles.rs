use crate::AtomicBucket;
use metrics::HistogramFn;
use quanta::Instant;

impl HistogramFn for AtomicBucket<(f64, Instant)> {
    fn record(&self, value: f64) {
        let now = Instant::now();
        self.push((value, now));
    }
}
