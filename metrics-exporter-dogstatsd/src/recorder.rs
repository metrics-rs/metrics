use std::sync::Arc;

use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};

use crate::state::State;

/// A recorder that forwards metrics to a DogStatsD server.
pub struct DogStatsDRecorder {
    state: Arc<State>,
}

impl DogStatsDRecorder {
    pub(crate) fn new(state: Arc<State>) -> Self {
        DogStatsDRecorder { state }
    }
}

impl Recorder for DogStatsDRecorder {
    fn describe_counter(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
    fn describe_gauge(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
    fn describe_histogram(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

    fn register_counter(&self, key: &Key, _: &Metadata<'_>) -> Counter {
        self.state
            .registry()
            .get_or_create_counter(key, |existing| Counter::from_arc(Arc::clone(existing)))
    }

    fn register_gauge(&self, key: &Key, _: &Metadata<'_>) -> Gauge {
        self.state
            .registry()
            .get_or_create_gauge(key, |existing| Gauge::from_arc(Arc::clone(existing)))
    }

    fn register_histogram(&self, key: &Key, _: &Metadata<'_>) -> Histogram {
        self.state
            .registry()
            .get_or_create_histogram(key, |existing| Histogram::from_arc(Arc::clone(existing)))
    }
}
