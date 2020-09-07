use metrics::{Identifier, Key, Label, Recorder};
use metrics_util::layers::Layer;
use tracing::Span;

mod tracing_integration;

pub use tracing_integration::{MetricsLayer, SpanExt};

pub struct TracingContextLayer {
    global_fields: Vec<String>,
}

impl TracingContextLayer {
    /// Creates a new `TracingContextLayer` without any fields enabled globally.
    ///
    /// Only dynamic fields specified at the callsite will be resolved when using the layer in this mode.
    pub fn new() -> Self {
        Self {
            global_fields: Vec::new(),
        }
    }

    /// Creates a new `TracingContextLayer` with the specified fields enabled globally.
    ///
    /// This will inject the specified fields, if they are present in the current span, into all
    /// metric label sets.  Fields are matched case-sensitively and in their entirety.
    pub fn with_global_fields(global_fields: Vec<String>) -> Self {
        Self {
            global_fields,
        }
    }
}

impl<R> Layer<R> for TracingContextLayer {
    type Output = TracingContext<R>;

    fn layer(&self, inner: R) -> Self::Output {
        TracingContext {
            inner,
            global_fields: self.global_fields.iter()
                .cloned()
                .map(Label::from_dynamic)
                .collect(),
        }
    }
}

pub struct TracingContext<R> {
    inner: R,
    global_fields: Vec<Label>,
}

impl<R> TracingContext<R> {
    fn enhance_key(&self, mut key: Key) -> Key {
        let span = Span::current();
        span.with_labels(|new_labels| {
            if !new_labels.is_empty() {
                let old_labels = key.labels_mut().get_or_insert_with(|| Vec::new());
                if !self.global_fields.is_empty() {
                    let global_fields = self.global_fields.clone();
                    for global_field in global_fields {
                        if old_labels.iter().position(|l| l.key() == global_field.key()).is_none() {
                            old_labels.push(global_field);
                        }
                    }
                }

                for old_label in old_labels.iter_mut() {
                    // Check if this label still needs to be given a value.
                    if old_label.requires_value() {
                        // Do our tracing fields have a match for this particular label?
                        for new_label in new_labels.iter() {
                            if new_label.0 == old_label.key() {
                                old_label.set_value(new_label.1.to_string());
                                break;
                            }
                        }
                    }
                }
            }
        });

        key
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