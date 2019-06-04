use crate::common::MetricValue;

/// Proxy object to update a gauge.
pub struct Gauge {
    handle: MetricValue,
}

impl Gauge {
    /// Records a value for the gauge.
    pub fn record(&self, value: i64) {
        self.handle.update_gauge(value);
    }
}

impl From<MetricValue> for Gauge {
    fn from(handle: MetricValue) -> Self {
        Self { handle }
    }
}
