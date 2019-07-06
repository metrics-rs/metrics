use crate::common::ValueHandle;

/// Proxy object to update a gauge.
pub struct Gauge {
    handle: ValueHandle,
}

impl Gauge {
    /// Records a value for the gauge.
    pub fn record(&self, value: i64) {
        self.handle.update_gauge(value);
    }
}

impl From<ValueHandle> for Gauge {
    fn from(handle: ValueHandle) -> Self {
        Self { handle }
    }
}
