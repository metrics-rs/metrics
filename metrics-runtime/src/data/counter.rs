use crate::common::ValueHandle;

/// A reference to a [`Counter`].
///
/// A [`Counter`] is used for directly updating a counter, without any lookup overhead.
#[derive(Clone)]
pub struct Counter {
    handle: ValueHandle,
}

impl Counter {
    /// Records a value for the counter.
    pub fn record(&self, value: u64) {
        self.handle.update_counter(value);
    }

    /// Increments the counter by one.
    pub fn increment(&self) {
        self.handle.update_counter(1);
    }
}

impl From<ValueHandle> for Counter {
    fn from(handle: ValueHandle) -> Self {
        Self { handle }
    }
}
