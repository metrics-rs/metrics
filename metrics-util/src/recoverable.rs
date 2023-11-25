use std::sync::{Arc, Weak};

use metrics::{
    Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SetRecorderError, SharedString,
    Unit,
};

pub struct RecoveryHandle<R> {
    handle: Arc<R>,
}

impl<R> RecoveryHandle<R> {
    /// Consumes the handle, returning the original recorder.
    ///
    /// This method will loop until there are no other strong references to the recorder. This means
    /// that the wrapped recorder which was installed is not being actively used, as using it
    /// temporarily upgrades its internal weak reference to a strong reference.
    ///
    /// It is not advised to call this method under heavy load, as doing so is not deterministic or
    /// ordered and may block for an indefinite amount of time.
    pub fn into_inner(mut self) -> R {
        loop {
            match Arc::try_unwrap(self.handle) {
                Ok(recorder) => break recorder,
                Err(handle) => {
                    self.handle = handle;
                }
            }
        }
    }
}

/// Wraps a recorder to allow for recovering it after being installed.
///
/// Installing a recorder generally involves providing an owned value, which means that it is not
/// possible to recover the recorder after it has been installed. For some recorder implementations,
/// it can be important to perform finalization before the application exits, which is not possible
/// if the application cannot consume the recorder.
///
/// `RecoverableRecorder` allows wrapping a recorder such that a weak reference to it is installed
/// globally, while the recorder itself is held by `RecoveryHandle<R>`. This allows the recorder to
/// be used globally so long as the recovery handle is active, keeping the original recorder alive.
///
/// ## As a drop guard
///
/// While `RecoveryHandle<R>` provides a method to manually recover the recorder directly, one
/// particular benefit is that due to how the recorder is wrapped, when `RecoveryHandle<R>` is
/// dropped, and the last active reference to the wrapped recorder is dropped, the recorder itself
/// will be dropped.
///
/// This allows using `RecoveryHandle<R>` as a drop guard, ensuring that by dropping it, the
/// recorder itself will be dropped, and any finalization logic implemented for the recorder will be
/// run.
pub struct RecoverableRecorder<R> {
    handle: Arc<R>,
}

impl<R: Recorder + 'static> RecoverableRecorder<R> {
    /// Creates a new `RecoverableRecorder` from the given recorder.
    pub fn new(recorder: R) -> Self {
        Self { handle: Arc::new(recorder) }
    }

    /// Builds the wrapped recorder and a handle to recover the original.
    pub(self) fn build(self) -> (WeakRecorder<R>, RecoveryHandle<R>) {
        let wrapped = WeakRecorder::from_arc(&self.handle);

        (wrapped, RecoveryHandle { handle: self.handle })
    }

    /// Installs the wrapped recorder globally, returning a handle to recover it.
    ///
    /// A weakly-referenced version of the recorder is installed globally, while the original
    /// recorder is held within `RecoverableRecorder`, and can be recovered by calling `into_inner`.
    ///
    /// # Errors
    ///
    /// If a recorder is already installed, an error is returned containing the original recorder.
    pub fn install(self) -> Result<RecoveryHandle<R>, SetRecorderError<R>> {
        let (wrapped, handle) = self.build();
        match metrics::set_global_recorder(wrapped) {
            Ok(()) => Ok(handle),
            Err(_) => {
                let recorder = handle.into_inner();
                Err(SetRecorderError(recorder))
            }
        }
    }
}

struct WeakRecorder<R> {
    recorder: Weak<R>,
}

impl<R> WeakRecorder<R> {
    fn from_arc(recorder: &Arc<R>) -> Self {
        Self { recorder: Arc::downgrade(recorder) }
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

    fn register_counter(&self, key: &Key, metadata: &Metadata<'_>) -> Counter {
        if let Some(recorder) = self.recorder.upgrade() {
            recorder.register_counter(key, metadata)
        } else {
            Counter::noop()
        }
    }

    fn register_gauge(&self, key: &Key, metadata: &Metadata<'_>) -> Gauge {
        if let Some(recorder) = self.recorder.upgrade() {
            recorder.register_gauge(key, metadata)
        } else {
            Gauge::noop()
        }
    }

    fn register_histogram(&self, key: &Key, metadata: &Metadata<'_>) -> Histogram {
        if let Some(recorder) = self.recorder.upgrade() {
            recorder.register_histogram(key, metadata)
        } else {
            Histogram::noop()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use metrics::{atomics::AtomicU64, CounterFn, GaugeFn, HistogramFn, Key, Recorder};

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
        dropped: Arc<AtomicBool>,
        counter: Arc<CounterWrapper>,
        gauge: Arc<GaugeWrapper>,
        histogram: Arc<HistogramWrapper>,
    }

    impl TestRecorder {
        fn new() -> (Self, Arc<CounterWrapper>, Arc<GaugeWrapper>, Arc<HistogramWrapper>) {
            let (recorder, _, counter, gauge, histogram) = Self::new_with_drop();
            (recorder, counter, gauge, histogram)
        }

        fn new_with_drop(
        ) -> (Self, Arc<AtomicBool>, Arc<CounterWrapper>, Arc<GaugeWrapper>, Arc<HistogramWrapper>)
        {
            let dropped = Arc::new(AtomicBool::new(false));
            let counter = Arc::new(CounterWrapper(AtomicU64::new(0)));
            let gauge = Arc::new(GaugeWrapper(AtomicU64::new(0)));
            let histogram = Arc::new(HistogramWrapper(AtomicU64::new(0)));

            let recorder = Self {
                dropped: Arc::clone(&dropped),
                counter: Arc::clone(&counter),
                gauge: Arc::clone(&gauge),
                histogram: Arc::clone(&histogram),
            };

            (recorder, dropped, counter, gauge, histogram)
        }
    }

    impl Recorder for TestRecorder {
        fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {
            todo!()
        }

        fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {
            todo!()
        }

        fn describe_histogram(
            &self,
            _key: KeyName,
            _unit: Option<Unit>,
            _description: SharedString,
        ) {
            todo!()
        }

        fn register_counter(&self, _: &Key, _: &Metadata<'_>) -> Counter {
            Counter::from_arc(Arc::clone(&self.counter))
        }

        fn register_gauge(&self, _: &Key, _: &Metadata<'_>) -> Gauge {
            Gauge::from_arc(Arc::clone(&self.gauge))
        }

        fn register_histogram(&self, _: &Key, _: &Metadata<'_>) -> Histogram {
            Histogram::from_arc(Arc::clone(&self.histogram))
        }
    }

    impl Drop for TestRecorder {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::Release);
        }
    }

    #[test]
    fn basic() {
        // Create and install the recorder.
        let (recorder, counter, gauge, histogram) = TestRecorder::new();
        let recoverable = RecoverableRecorder::new(recorder);
        let (recorder, handle) = recoverable.build();

        // Record some metrics, and make sure the atomics for each metric type are
        // incremented as we would expect them to be.
        metrics::with_local_recorder(&recorder, || {
            metrics::counter!("counter").increment(5);
            metrics::gauge!("gauge").increment(5.0);
            metrics::gauge!("gauge").increment(5.0);
            metrics::histogram!("histogram").record(5.0);
            metrics::histogram!("histogram").record(5.0);
            metrics::histogram!("histogram").record(5.0);
        });

        let _recorder = handle.into_inner();
        assert_eq!(counter.get(), 5);
        assert_eq!(gauge.get(), 10);
        assert_eq!(histogram.get(), 15);

        // Now that we've recovered the recorder, incrementing the same metrics should
        // not actually increment the value of the atomics for each metric type.
        metrics::with_local_recorder(&recorder, || {
            metrics::counter!("counter").increment(7);
            metrics::gauge!("gauge").increment(7.0);
            metrics::histogram!("histogram").record(7.0);
        });

        assert_eq!(counter.get(), 5);
        assert_eq!(gauge.get(), 10);
        assert_eq!(histogram.get(), 15);
    }

    #[test]
    fn on_drop() {
        // Create and install the recorder.
        let (recorder, dropped, counter, gauge, histogram) = TestRecorder::new_with_drop();
        let recoverable = RecoverableRecorder::new(recorder);
        let (recorder, handle) = recoverable.build();

        // Record some metrics, and make sure the atomics for each metric type are
        // incremented as we would expect them to be.
        metrics::with_local_recorder(&recorder, || {
            metrics::counter!("counter").increment(5);
            metrics::gauge!("gauge").increment(5.0);
            metrics::gauge!("gauge").increment(5.0);
            metrics::histogram!("histogram").record(5.0);
            metrics::histogram!("histogram").record(5.0);
            metrics::histogram!("histogram").record(5.0);
        });

        drop(handle.into_inner());
        assert_eq!(counter.get(), 5);
        assert_eq!(gauge.get(), 10);
        assert_eq!(histogram.get(), 15);

        // Now that we've recovered the recorder, incrementing the same metrics should
        // not actually increment the value of the atomics for each metric type.
        metrics::with_local_recorder(&recorder, || {
            metrics::counter!("counter").increment(7);
            metrics::gauge!("gauge").increment(7.0);
            metrics::histogram!("histogram").record(7.0);
        });

        assert_eq!(counter.get(), 5);
        assert_eq!(gauge.get(), 10);
        assert_eq!(histogram.get(), 15);

        // And we should be able to check that the recorder was indeed dropped.
        assert!(dropped.load(Ordering::Acquire));
    }
}
