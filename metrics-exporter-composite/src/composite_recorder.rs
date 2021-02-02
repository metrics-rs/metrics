use metrics::{GaugeValue, Key, Recorder, SetRecorderError, Unit};

pub struct CompositeRecorderBuilder {
    inner: Vec<Box<dyn Recorder + 'static>>,
}

pub struct CompositeRecorder {
    inner: Vec<Box<dyn Recorder + 'static>>,
}

impl CompositeRecorderBuilder {
    pub fn add_recorder<R>(mut self, recorder: R) -> Self
    where
        R: Recorder + 'static,
    {
        self.inner.push(Box::new(recorder));
        self
    }

    pub fn new() -> CompositeRecorderBuilder {
        CompositeRecorderBuilder { inner: Vec::new() }
    }

    pub fn build(self) -> CompositeRecorder {
        CompositeRecorder { inner: self.inner }
    }

    pub fn install(self) -> Result<(), SetRecorderError> {
        let recorder = self.build();
        metrics::set_boxed_recorder(Box::new(recorder))
    }
}

impl Recorder for CompositeRecorder {
    fn register_counter(&self, key: Key, unit: Option<Unit>, description: Option<&'static str>) {
        self.inner.iter().for_each(|recorder| {
            recorder.register_counter(key.clone(), unit.clone(), description.clone())
        });
    }

    fn register_gauge(&self, key: Key, unit: Option<Unit>, description: Option<&'static str>) {
        self.inner.iter().for_each(|recorder| {
            recorder.register_gauge(key.clone(), unit.clone(), description.clone())
        });
    }

    fn register_histogram(&self, key: Key, unit: Option<Unit>, description: Option<&'static str>) {
        self.inner.iter().for_each(|recorder| {
            recorder.register_histogram(key.clone(), unit.clone(), description.clone())
        });
    }

    fn increment_counter(&self, key: Key, value: u64) {
        self.inner
            .iter()
            .for_each(|recorder| recorder.increment_counter(key.clone(), value.clone()));
    }

    fn update_gauge(&self, key: Key, value: GaugeValue) {
        self.inner
            .iter()
            .for_each(|recorder| recorder.update_gauge(key.clone(), value.clone()));
    }

    fn record_histogram(&self, key: Key, value: f64) {
        self.inner
            .iter()
            .for_each(|recorder| recorder.record_histogram(key.clone(), value.clone()));
    }
}

#[cfg(test)]
mod tests {
    use mockall::mock;
    use mockall::predicate::eq;

    use metrics::KeyData;

    use super::*;

    mock! {
        pub TestRecorder{}
        impl Recorder for TestRecorder {
            fn register_counter(&self, key: Key, unit: Option<Unit>, description: Option<&'static str>);
            fn register_gauge(&self, key: Key, unit: Option<Unit>, description: Option<&'static str>);
            fn register_histogram(&self, key: Key, unit: Option<Unit>, description: Option<&'static str>);
            fn increment_counter(&self, key: Key, value: u64);
            fn update_gauge(&self, key: Key, value: GaugeValue);
            fn record_histogram(&self, key: Key, value: f64);
        }
    }

    #[test]
    pub fn test_install() {
        assert!(metrics::try_recorder().is_none());

        let base_recorder = MockTestRecorder::new();
        CompositeRecorderBuilder::new()
            .add_recorder(base_recorder)
            .install()
            .expect("Should install");

        assert!(metrics::try_recorder().is_some());
    }

    #[test]
    fn test_register_counter() {
        let key = Key::from(KeyData::from_name("counter"));
        let mut base_recorder = MockTestRecorder::new();
        base_recorder
            .expect_register_counter()
            .times(1)
            .with(eq(key.clone()), eq(None), eq(None))
            .return_const(());
        let composite = CompositeRecorderBuilder::new()
            .add_recorder(base_recorder)
            .build();

        composite.register_counter(key.clone(), None, None);
    }

    #[test]
    fn test_increment_counter() {
        let key = Key::from(KeyData::from_name("counter"));
        let mut base_recorder = MockTestRecorder::new();
        base_recorder.expect_register_counter().return_const(());
        base_recorder
            .expect_increment_counter()
            .times(1)
            .with(eq(key.clone()), eq(42))
            .return_const(());
        let composite = CompositeRecorderBuilder::new()
            .add_recorder(base_recorder)
            .build();

        composite.register_counter(key.clone(), None, None);
        composite.increment_counter(key.clone(), 42);
    }

    #[test]
    fn test_register_gauge() {
        let key = Key::from(KeyData::from_name("gauge"));
        let mut base_recorder = MockTestRecorder::new();
        base_recorder
            .expect_register_gauge()
            .times(1)
            .with(eq(key.clone()), eq(None), eq(None))
            .return_const(());
        let composite = CompositeRecorderBuilder::new()
            .add_recorder(base_recorder)
            .build();

        composite.register_gauge(key.clone(), None, None);
    }

    #[test]
    fn test_update_gauge() {
        let key = Key::from(KeyData::from_name("counter"));
        let mut base_recorder = MockTestRecorder::new();
        base_recorder.expect_register_gauge().return_const(());
        base_recorder
            .expect_update_gauge()
            .times(1)
            .with(eq(key.clone()), eq(GaugeValue::Absolute(42.0)))
            .return_const(());
        let composite = CompositeRecorderBuilder::new()
            .add_recorder(base_recorder)
            .build();

        composite.register_gauge(key.clone(), None, None);
        composite.update_gauge(key.clone(), GaugeValue::Absolute(42.0));
    }

    #[test]
    fn test_register_histogram() {
        let key = Key::from(KeyData::from_name("histogram"));
        let mut base_recorder = MockTestRecorder::new();
        base_recorder
            .expect_register_histogram()
            .times(1)
            .with(eq(key.clone()), eq(None), eq(None))
            .return_const(());
        let composite = CompositeRecorderBuilder::new()
            .add_recorder(base_recorder)
            .build();

        composite.register_histogram(key.clone(), None, None);
    }

    #[test]
    fn test_record_histogram() {
        let key = Key::from(KeyData::from_name("counter"));
        let mut base_recorder = MockTestRecorder::new();
        base_recorder.expect_register_histogram().return_const(());
        base_recorder
            .expect_record_histogram()
            .times(1)
            .with(eq(key.clone()), eq(2.0))
            .return_const(());
        let composite = CompositeRecorderBuilder::new()
            .add_recorder(base_recorder)
            .build();

        composite.register_histogram(key.clone(), None, None);
        composite.record_histogram(key.clone(), 2.0);
    }
}
