use metrics::{Identifier, Key, Recorder};
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
    fn enhance_key(&self, key: Key) -> Key {
        let span = Span::current();
        let (scope_name, mut labels) = key.into_parts();
        span.with_labels(|new_labels| {
            if !new_labels.is_empty() {
                labels
                    .get_or_insert_with(|| Vec::new())
                    .extend_from_slice(&new_labels);
            }
        });
        match labels {
            Some(labels) => Key::from_name_and_labels(scope_name, labels),
            None => Key::from_name(scope_name),
        }
    }
}

impl<R: Recorder> Recorder for TracingContext<R> {
    fn register_counter(&self, key: Key, description: Option<&'static str>) -> Identifier {
        let new_key = self.enhance_key(key);
        self.inner.register_counter(new_key, description)
    }

    fn register_gauge(&self, key: Key, description: Option<&'static str>) -> Identifier {
        let new_key = self.enhance_key(key);
        self.inner.register_gauge(new_key, description)
    }

    fn register_histogram(&self, key: Key, description: Option<&'static str>) -> Identifier {
        let new_key = self.enhance_key(key);
        self.inner.register_histogram(new_key, description)
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
}
