use std::sync::{Arc, Weak};

use metrics::{Recorder, KeyName, Unit, SharedString, Key, Counter, Gauge, Histogram, SetRecorderError};

/// Wraps a recorder to allow for recovering it after being installed.
///
/// Installing a recorder generally involves providing an owned value, which means that it is not
/// possible to recover the recorder after it has been installed. For some recorder implementations,
/// it can be important to perform finalization before the application exits, which is not possible
/// if the application cannot consume the recorder.
///
/// `RecoverableRecorder` allows wrapping a recorder such that a weak reference to it is installed
/// globally, while the recorder itself is held by `RecoverableRecorder`. This allows for recovering
/// the recorder whenever the application chooses.
pub struct RecoverableRecorder<R> {
    recorder: Arc<R>,
}

impl<R: Recorder + 'static> RecoverableRecorder<R> {
    /// Creates a new `RecoverableRecorder` wrapper around the given recorder.
    ///
    /// This also installs the recorder globally, returning an error if there was already a recorder
    /// installed.
    pub fn from_recorder(recorder: R) -> Result<Self, SetRecorderError> {
        let recorder = Arc::new(recorder);

        let wrapped = WeakRecorder::from_arc(&recorder);
        metrics::set_boxed_recorder(Box::new(wrapped))?;

        Ok(Self { recorder })
    }

    /// Consumes this wrapper, returning the wrapped recorder.
    ///
    /// This method will loop until there are no active weak references to the recorder. It is not
    /// advised to call this method under heavy load, as doing so is not deterministic or ordered
    /// and may block for an indefinite amount of time.
    pub fn into_inner(mut self) -> R {
        loop {
            match Arc::try_unwrap(self.recorder) {
                Ok(recorder) => break recorder,
                Err(recorder) => {
                    self.recorder = recorder;
                }
            }
        }
    }
}

struct WeakRecorder<R> {
    recorder: Weak<R>,
}

impl<R> WeakRecorder<R> {
    fn from_arc(recorder: &Arc<R>) -> Self {
        Self {
            recorder: Arc::downgrade(recorder),
        }
    }
}

impl<R: Recorder> Recorder for WeakRecorder<R> {
    fn describe_counter(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        if let Some(recorder) = self.recorder.upgrade() {
            recorder.describe_counter(key, unit, description);
        }
    }

    fn describe_gauge(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        if let Some(recorder) = self.recorder.upgrade() {
            recorder.describe_gauge(key, unit, description);
        }
    }

    fn describe_histogram(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        if let Some(recorder) = self.recorder.upgrade() {
            recorder.describe_histogram(key, unit, description);
        }
    }

    fn register_counter(&self, key: &Key) -> Counter {
        if let Some(recorder) = self.recorder.upgrade() {
            recorder.register_counter(key)
        } else {
            Counter::noop()
        }
    }

    fn register_gauge(&self, key: &Key) -> Gauge {
        if let Some(recorder) = self.recorder.upgrade() {
            recorder.register_gauge(key)
        } else {
            Gauge::noop()
        }
    }

    fn register_histogram(&self, key: &Key) -> Histogram {
        if let Some(recorder) = self.recorder.upgrade() {
            recorder.register_histogram(key)
        } else {
            Histogram::noop()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;
    use metrics::{Key, Recorder, atomics::AtomicU64, CounterFn, GaugeFn, HistogramFn};

    struct CounterWrapper(AtomicU64);
    struct GaugeWrapper(AtomicU64);
    struct HistogramWrapper(AtomicU64);

    impl CounterWrapper {
        fn get(&self) -> u64 {
            self.0.load(Ordering::Acquire)
        }
    }

    impl GaugeWrapper {
        fn get(&self) -> u64 {
            self.0.load(Ordering::Acquire)
        }
    }

    impl HistogramWrapper {
        fn get(&self) -> u64 {
            self.0.load(Ordering::Acquire)
        }
    }

    impl CounterFn for CounterWrapper {
        fn increment(&self, value: u64) {
            self.0.fetch_add(value, Ordering::Release);
        }

        fn absolute(&self, value: u64) {
            self.0.store(value, Ordering::Release);
        }
    }

    impl GaugeFn for GaugeWrapper {
        fn increment(&self, value: f64) {
            self.0.fetch_add(value as u64, Ordering::Release);
        }

        fn decrement(&self, value: f64) {
            self.0.fetch_sub(value as u64, Ordering::Release);
        }

        fn set(&self, value: f64) {
            self.0.store(value as u64, Ordering::Release);
        }
    }

    impl HistogramFn for HistogramWrapper {
        fn record(&self, value: f64) {
            self.0.fetch_add(value as u64, Ordering::Release);
        }
    }

    struct TestRecorder {
        counter: Arc<CounterWrapper>,
        gauge: Arc<GaugeWrapper>,
        histogram: Arc<HistogramWrapper>,
    }

    impl TestRecorder {
        fn new() -> (Self, Arc<CounterWrapper>, Arc<GaugeWrapper>, Arc<HistogramWrapper>) {
            let counter = Arc::new(CounterWrapper(AtomicU64::new(0)));
            let gauge = Arc::new(GaugeWrapper(AtomicU64::new(0)));
            let histogram = Arc::new(HistogramWrapper(AtomicU64::new(0)));

            let recorder = Self {
                counter: Arc::clone(&counter),
                gauge: Arc::clone(&gauge),
                histogram: Arc::clone(&histogram),
            };

            (recorder, counter, gauge, histogram)
        }
    }

    impl Recorder for TestRecorder {
        fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {
            todo!()
        }

        fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {
            todo!()
        }

        fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {
            todo!()
        }

        fn register_counter(&self, _: &Key) -> Counter {
            Counter::from_arc(Arc::clone(&self.counter))
        }

        fn register_gauge(&self, _: &Key) -> Gauge {
            Gauge::from_arc(Arc::clone(&self.gauge))
        }

        fn register_histogram(&self, _: &Key) -> Histogram {
            Histogram::from_arc(Arc::clone(&self.histogram))
        }
    }

    #[test]
    fn basic() {
        // Create and install the recorder.
        let (recorder, counter, gauge, histogram) = TestRecorder::new();
        let recoverable = RecoverableRecorder::from_recorder(recorder)
            .expect("failed to install recorder");

        // Record some metrics, and make sure the atomics for each metric type are
        // incremented as we would expect them to be.
        metrics::counter!("counter", 5);
        metrics::increment_gauge!("gauge", 5.0);
        metrics::increment_gauge!("gauge", 5.0);
        metrics::histogram!("histogram", 5.0);
        metrics::histogram!("histogram", 5.0);
        metrics::histogram!("histogram", 5.0);

        let _recorder = recoverable.into_inner();
        assert_eq!(counter.get(), 5);
        assert_eq!(gauge.get(), 10);
        assert_eq!(histogram.get(), 15);

        // Now that we've recovered the recorder, incrementing the same metrics should
        // not actually increment the value of the atomics for each metric type.
        metrics::counter!("counter", 7);
        metrics::increment_gauge!("gauge", 7.0);
        metrics::histogram!("histogram", 7.0);

        assert_eq!(counter.get(), 5);
        assert_eq!(gauge.get(), 10);
        assert_eq!(histogram.get(), 15);
    }
}
