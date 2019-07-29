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
}

impl From<ValueHandle> for Gauge {
    fn from(handle: ValueHandle) -> Self {
        Self { handle }
    }
}
