use crate::common::ValueHandle;

/// A reference to a [`Gauge`].
///
/// A [`Gauge`] is used for directly updating a gauge, without any lookup overhead.
#[derive(Clone)]
pub struct Gauge {
    handle: ValueHandle,
}

impl Gauge {
    /// Records a value for the gauge.
    pub fn record(&self, value: i64) {
        self.handle.update_gauge(value);
    }

    /// Increments the gauge's value
    pub fn increment(&self, value: i64) {
        self.handle.increment_gauge(value);
    }

    /// Decrements the gauge's value
    pub fn decrement(&self, value: i64) {
        self.handle.decrement_gauge(value);
    }
}

impl From<ValueHandle> for Gauge {
    fn from(handle: ValueHandle) -> Self {
        Self { handle }
    }
}
