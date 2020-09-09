use metrics::{Identifier, Key, Label, Recorder};
use metrics_util::layers::Layer;
use tracing::Span;

mod tracing_integration;

pub use tracing_integration::{MetricsLayer, SpanExt};

pub struct TracingContextLayer;

impl<R> Layer<R> for TracingContextLayer {
    type Output = TracingContext<R>;

    fn layer(&self, inner: R) -> Self::Output {
        TracingContext { inner }
    }
}

pub struct TracingContext<R> {
    inner: R,
}

impl<R> TracingContext<R> {
    fn enhance_dynamic_labels(&self, labels: &mut Vec<Label>) {
        let span = Span::current();
        span.with_labels(|new_labels| {
            labels.extend_from_slice(&new_labels);
        });
    }
}

impl<R: Recorder> Recorder for TracingContext<R> {
    fn register_counter(&self, key: Key, description: Option<&'static str>) -> Identifier {
        self.inner.register_counter(key, description)
    }

    fn register_gauge(&self, key: Key, description: Option<&'static str>) -> Identifier {
        self.inner.register_gauge(key, description)
    }

    fn register_histogram(&self, key: Key, description: Option<&'static str>) -> Identifier {
        self.inner.register_histogram(key, description)
    }

    fn increment_counter(&self, id: Identifier, value: u64) {
        self.inner.increment_counter(id, value);
    }

    fn update_gauge(&self, id: Identifier, value: f64) {
        self.inner.update_gauge(id, value);
    }

    fn record_histogram(&self, id: Identifier, value: u64) {
        self.inner.record_histogram(id, value);
    }

    fn increment_dynamic_counter(&self, key: Key, value: u64, mut dynamic_labels: Vec<Label>) {
        self.enhance_dynamic_labels(&mut dynamic_labels);
        self.inner
            .increment_dynamic_counter(key, value, dynamic_labels);
    }

    fn update_dynamic_gauge(&self, key: Key, value: f64, mut dynamic_labels: Vec<Label>) {
        self.enhance_dynamic_labels(&mut dynamic_labels);
        self.inner.update_dynamic_gauge(key, value, dynamic_labels);
    }

    fn record_dynamic_histogram(&self, key: Key, value: u64, mut dynamic_labels: Vec<Label>) {
        self.enhance_dynamic_labels(&mut dynamic_labels);
        self.inner
            .record_dynamic_histogram(key, value, dynamic_labels);
    }
}
