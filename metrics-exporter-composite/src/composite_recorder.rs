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
