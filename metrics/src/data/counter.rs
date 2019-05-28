use crate::common::MetricValue;

/// Proxy object to update a counter.
pub struct Counter {
    handle: MetricValue,
}

impl Counter {
    /// Records a value for the counter.
    pub fn record(&self, value: u64) {
        self.handle.update_counter(value);
    }
}

impl From<MetricValue> for Counter {
    fn from(handle: MetricValue) -> Self {
        Self { handle }
    }
}
