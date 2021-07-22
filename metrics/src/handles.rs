use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use crate::IntoF64;

/// A counter handler.
pub trait CounterFn {
    /// Increments the counter by the given amount.
    ///
    /// Returns the previous value.
    fn increment(&self, value: u64) -> u64;

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
    ///
    /// Returns the previous value.
    fn absolute(&self, value: u64) -> u64;
}

/// A gauge handler.
pub trait GaugeFn {
    /// Increments the gauge by the given amount.
    ///
    /// Returns the previous value.
    fn increment(&self, value: f64) -> f64;

    /// Decrements the gauge by the given amount.
    ///
    /// Returns the previous value.
    fn decrement(&self, value: f64) -> f64;

    /// Sets the gauge to the given amount.
    ///
    /// Returns the previous value.
    fn set(&self, value: f64) -> f64;
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
#[derive(Clone)]
pub struct Histogram {
    inner: Option<Arc<dyn HistogramFn>>,
}

impl Counter {
    /// Creates a no-op `Counter` which does nothing.
    ///
    /// Suitable when a handle must be provided that does nothing i.e. a no-op recorder or a layer
    /// that disables specific metrics, and so on.
    pub fn noop() -> Self {
        Self { inner: None }
    }

    /// Creates a `Counter` based on a shared handler.
    pub fn from_arc<F: CounterFn + 'static>(a: Arc<F>) -> Self {
        Self { inner: Some(a) }
    }

    /// Increments the counter.
    pub fn increment(&self, value: u64) -> u64 {
        self.inner
            .as_ref()
            .map(|c| c.increment(value))
            .unwrap_or_default()
    }

    /// Sets the counter to an absolute value.
    pub fn absolute(&self, value: u64) -> u64 {
        self.inner
            .as_ref()
            .map(|c| c.absolute(value))
            .unwrap_or_default()
    }
}

impl Gauge {
    /// Creates a no-op `Gauge` which does nothing.
    ///
    /// Suitable when a handle must be provided that does nothing i.e. a no-op recorder or a layer
    /// that disables specific metrics, and so on.
    pub fn noop() -> Self {
        Self { inner: None }
    }

    /// Creates a `Gauge` based on a shared handler.
    pub fn from_arc<F: GaugeFn + 'static>(a: Arc<F>) -> Self {
        Self { inner: Some(a) }
    }

    /// Increments the gauge.
    pub fn increment<T: IntoF64>(&self, value: T) -> f64 {
        self.inner
            .as_ref()
            .map(|g| g.increment(value.into_f64()))
            .unwrap_or_default()
    }

    /// Decrements the gauge.
    pub fn decrement<T: IntoF64>(&self, value: T) -> f64 {
        self.inner
            .as_ref()
            .map(|g| g.decrement(value.into_f64()))
            .unwrap_or_default()
    }

    /// Sets the gauge.
    pub fn set<T: IntoF64>(&self, value: T) -> f64 {
        self.inner
            .as_ref()
            .map(|g| g.set(value.into_f64()))
            .unwrap_or_default()
    }
}

impl Histogram {
    /// Creates a no-op `Histogram` which does nothing.
    ///
    /// Suitable when a handle must be provided that does nothing i.e. a no-op recorder or a layer
    /// that disables specific metrics, and so on.
    pub fn noop() -> Self {
        Self { inner: None }
    }

    /// Creates a `Histogram` based on a shared handler.
    pub fn from_arc<F: HistogramFn + 'static>(a: Arc<F>) -> Self {
        Self { inner: Some(a) }
    }

    /// Records a value in the histogram.
    pub fn record<T: IntoF64>(&self, value: T) {
        if let Some(ref inner) = self.inner {
            inner.record(value.into_f64())
        }
    }
}

impl CounterFn for AtomicU64 {
    fn increment(&self, value: u64) -> u64 {
        self.fetch_add(value, Ordering::Release)
    }

    fn absolute(&self, value: u64) -> u64 {
        self.fetch_max(value, Ordering::AcqRel)
    }
}

impl GaugeFn for AtomicU64 {
    fn increment(&self, value: f64) -> f64 {
        loop {
            let result = self.fetch_update(Ordering::AcqRel, Ordering::Relaxed, |curr| {
                let input = f64::from_bits(curr);
                let output = input + value;
                Some(output.to_bits())
            });

            if let Ok(previous) = result {
                return f64::from_bits(previous);
            }
        }
    }

    fn decrement(&self, value: f64) -> f64 {
        loop {
            let result = self.fetch_update(Ordering::AcqRel, Ordering::Relaxed, |curr| {
                let input = f64::from_bits(curr);
                let output = input - value;
                Some(output.to_bits())
            });

            if let Ok(previous) = result {
                return f64::from_bits(previous);
            }
        }
    }

    fn set(&self, value: f64) -> f64 {
        f64::from_bits(self.swap(value.to_bits(), Ordering::AcqRel))
    }
}

impl<T> CounterFn for Arc<T>
where
    T: CounterFn,
{
    fn increment(&self, value: u64) -> u64 {
        (**self).increment(value)
    }

    fn absolute(&self, value: u64) -> u64 {
        (**self).absolute(value)
    }
}
impl<T> GaugeFn for Arc<T>
where
    T: GaugeFn,
{
    fn increment(&self, value: f64) -> f64 {
        (**self).increment(value)
    }

    fn decrement(&self, value: f64) -> f64 {
        (**self).decrement(value)
    }

    fn set(&self, value: f64) -> f64 {
        (**self).set(value)
    }
}

impl<T> HistogramFn for Arc<T>
where
    T: HistogramFn,
{
    fn record(&self, value: f64) {
        (**self).record(value);
    }
}
