use metrics::{Key, KeyRef, Label, Recorder};
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
    fn enhance_labels(&self, labels: &mut Vec<Label>) {
        let span = Span::current();
        span.with_labels(|new_labels| {
            labels.extend_from_slice(&new_labels);
        });
    }

    fn enhance_key(&self, key: KeyRef) -> KeyRef {
        let (name, mut labels) = key.into_owned().into_parts();
        self.enhance_labels(&mut labels);
        Key::from_name_and_labels(name, labels).into()
    }
}

impl<R: Recorder> Recorder for TracingContext<R> {
    fn register_counter(&self, key: KeyRef, description: Option<&'static str>) {
        self.inner.register_counter(key, description)
    }

    fn register_gauge(&self, key: KeyRef, description: Option<&'static str>) {
        self.inner.register_gauge(key, description)
    }

    fn register_histogram(&self, key: KeyRef, description: Option<&'static str>) {
        self.inner.register_histogram(key, description)
    }

    fn increment_counter(&self, key: KeyRef, value: u64) {
        let key = self.enhance_key(key);
        self.inner.increment_counter(key, value);
    }

    fn update_gauge(&self, key: KeyRef, value: f64) {
        let key = self.enhance_key(key);
        self.inner.update_gauge(key, value);
    }

    fn record_histogram(&self, key: KeyRef, value: u64) {
        let key = self.enhance_key(key);
        self.inner.record_histogram(key, value);
    }
}
