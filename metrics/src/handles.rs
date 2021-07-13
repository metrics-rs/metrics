use std::sync::{Arc, atomic::{AtomicU64, Ordering}};

use crate::IntoF64;

/// A counter handler.
pub trait CounterFn {
    /// Increments the counter by the given amount.
    fn increment(&self, value: u64);

    /// Sets the counter to at least the given amount.
    ///
    /// This is intended to support use cases where multiple callers are attempting to synchronize
    /// this counter with an external counter that they have no control over.  As multiple callers
    /// may read that external counter, and attempt to set it here, there could be reordering issues
    /// where a caller attempts to set an older (smaller) value after the counter has been updated to
    /// the latest (larger) value.
    ///
    /// This method must cope with those cases.  An example of doing so atomically can be found in
    /// `AtomicCounter`.
    fn absolute(&self, value: u64);
}

/// A gauge handler.
pub trait GaugeFn {
    /// Increments the gauge by the given amount.
    fn increment(&self, value: f64);

    /// Decrements the gauge by the given amount.
    fn decrement(&self, value: f64);

    /// Sets the gauge to the given amount.
    fn set(&self, value: f64);
}

/// A histogram handler.
pub trait HistogramFn {
    /// Records a value into the histogram.
    fn record(&self, value: f64);
}

/// A counter.
#[derive(Clone)]
pub struct Counter {
    inner: Option<Arc<dyn CounterFn>>,
}

/// A gauge.
#[derive(Clone)]
pub struct Gauge {
    inner: Option<Arc<dyn GaugeFn>>,
}

/// A histogram.
pub struct Histogram {
    inner: Option<Arc<dyn HistogramFn>>,
}

impl Counter {
    /// Creates a no-op `Counter` which does nothing.
    ///
    /// Suitable when a handle must be provided that does nothing i.e. a no-op recorder or a layer
    /// that disables specific metrics, and so on.
    pub fn noop() -> Self {
        Self {
            inner: None,
        }
    }

    /// Creates a `Counter` based on a shared handler.
    pub fn from_arc<F: CounterFn + 'static>(a: Arc<F>) -> Self {
        Self {
            inner: Some(a),
        }
    }

    /// Increments the counter.
    pub fn increment(&self, value: u64) {
        if let Some(ref inner) = self.inner {
            inner.increment(value)
        }
    }

    /// Sets the counter to an absolute value.
    pub fn absolute(&self, value: u64) {
        if let Some(ref inner) = self.inner {
            inner.absolute(value)
        }
    }
}

impl Gauge {
    /// Creates a no-op `Gauge` which does nothing.
    ///
    /// Suitable when a handle must be provided that does nothing i.e. a no-op recorder or a layer
    /// that disables specific metrics, and so on.
    pub fn noop() -> Self {
        Self {
            inner: None,
        }
    }

    /// Creates a `Gauge` based on a shared handler.
    pub fn from_arc<F: GaugeFn + 'static>(a: Arc<F>) -> Self {
        Self {
            inner: Some(a),
        }
    }

    /// Increments the gauge.
    pub fn increment<T: IntoF64>(&self, value: T) {
        if let Some(ref inner) = self.inner {
            inner.increment(value.into_f64())
        }
    }

    /// Decrements the gauge.
    pub fn decrement<T: IntoF64>(&self, value: T) {
        if let Some(ref inner) = self.inner {
            inner.decrement(value.into_f64())
        }
    }

    /// Sets the gauge.
    pub fn set<T: IntoF64>(&self, value: T) {
        if let Some(ref inner) = self.inner {
            inner.set(value.into_f64())
        }
    }
}

impl Histogram {
    /// Creates a no-op `Histogram` which does nothing.
    ///
    /// Suitable when a handle must be provided that does nothing i.e. a no-op recorder or a layer
    /// that disables specific metrics, and so on.
    pub fn noop() -> Self {
        Self {
            inner: None,
        }
    }

    /// Creates a `Histogram` based on a shared handler.
    pub fn from_arc<F: HistogramFn + 'static>(a: Arc<F>) -> Self {
        Self {
            inner: Some(a),
        }
    }

    /// Records a value in the histogram.
    pub fn record<T: IntoF64>(&self, value: T) {
        if let Some(ref inner) = self.inner {
            inner.record(value.into_f64())
        }
    }
}

impl CounterFn for AtomicU64 {
    fn increment(&self, value: u64) {
        let _ = self.fetch_add(value, Ordering::Release);
    }

    fn absolute(&self, value: u64) {
        let _ = self.fetch_max(value, Ordering::AcqRel);
    }
}

impl GaugeFn for AtomicU64 {
    fn increment(&self, value: f64) {
        let _ = self.fetch_update(Ordering::AcqRel, Ordering::Relaxed, |curr| {
            let input = f64::from_bits(curr);
            let output = input + value;
            Some(output.to_bits())
        });
    }

    fn decrement(&self, value: f64) {
        let _ = self.fetch_update(Ordering::AcqRel, Ordering::Relaxed, |curr| {
            let input = f64::from_bits(curr);
            let output = input - value;
            Some(output.to_bits())
        });
    }

    fn set(&self, value: f64) {
        let _ = self.fetch_update(Ordering::AcqRel, Ordering::Relaxed, |_| {
            Some(value.to_bits())
        });
    }
}
