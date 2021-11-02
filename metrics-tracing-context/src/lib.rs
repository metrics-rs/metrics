//! Use [`tracing::span!`] fields as [`metrics`] labels.
//!
//! The `metrics-tracing-context` crate provides tools to enable injecting the
//! contextual data maintained via `span!` macro from the [`tracing`] crate
//! into the metrics.
//!
//! # Usage
//!
//! First, set up `tracing` and `metrics` crates:
//!
//! ```rust
//! # use metrics_util::DebuggingRecorder;
//! # use tracing_subscriber::Registry;
//! use metrics_util::layers::Layer;
//! use tracing_subscriber::layer::SubscriberExt;
//! use metrics_tracing_context::{MetricsLayer, TracingContextLayer};
//!
//! // Prepare tracing.
//! # let my_subscriber = Registry::default();
//! let subscriber = my_subscriber.with(MetricsLayer::new());
//! tracing::subscriber::set_global_default(subscriber).unwrap();
//!
//! // Prepare metrics.
//! # let my_recorder = DebuggingRecorder::new();
//! let recorder = TracingContextLayer::all().layer(my_recorder);
//! metrics::set_boxed_recorder(Box::new(recorder)).unwrap();
//! ```
//!
//! Then emit some metrics within spans and see the labels being injected!
//!
//! ```rust
//! # use metrics_util::{layers::Layer, DebuggingRecorder};
//! # use tracing_subscriber::{layer::SubscriberExt, Registry};
//! # use metrics_tracing_context::{MetricsLayer, TracingContextLayer};
//! # let mysubscriber = Registry::default();
//! # let subscriber = mysubscriber.with(MetricsLayer::new());
//! # tracing::subscriber::set_global_default(subscriber).unwrap();
//! # let myrecorder = DebuggingRecorder::new();
//! # let recorder = TracingContextLayer::all().layer(myrecorder);
//! # metrics::set_boxed_recorder(Box::new(recorder)).unwrap();
//! use tracing::{span, Level};
//! use metrics::counter;
//!
//! let user = "ferris";
//! let span = span!(Level::TRACE, "login", user);
//! let _guard = span.enter();
//!
//! counter!("login_attempts", 1, "service" => "login_service");
//! ```
//!
//! The code above will emit a increment for a `login_attempts` counter with
//! the following labels:
//! - `service=login_service`
//! - `user=ferris`
//!
//! # Implementation
//!
//! The integration layer works by capturing all fields present when a span is created and storing
//! them as an extension to the span.  If a metric is emitted while a span is entered, we check that
//! span to see if it has any fields in the extension data, and if it does, we add those fields as
//! labels to the metric key.
//!
//! There are two important behaviors to be aware of:
//! - we only capture the fields present when the span is created
//! - we store all fields that a span has, including the fields of its parent span(s)
//!
//! ## Lack of dynamism
//!
//! This means that if you use [`Span::record`][tracing::Span::record] to add fields to a span after
//! it has been created, those fields will not be captured and added to your metric key.
//!
//! ## Span fields and ancestry
//!
//! Likewise, we capture the sum of all fields for a span and its parent span(s), meaning that if you have the
//! following span stack:
//!
//! ```text
//! root span        (fieldA => valueA)
//!  ⤷ mid-tier span (fieldB => valueB)
//!     ⤷ leaf span  (fieldC => valueC)
//! ```
//!
//! Then a metric emitted while within the leaf span would get, as labels, all three fields: A, B,
//! and C.  As well, this layer does _not_ deduplicate the fields.  If you have two instance of the
//! same field name, both versions will be included in your metric labels.  Whether or not those are
//! deduplicated, and how they're deduplicated, is an exporter-specific implementation detail.
//!
//! In addition, for performance purposes, span fields are held in pooled storage, and additionally
//! will copy the fields of parent spans.  Following the example span stack from above, the mid-tier
//! span would hold both field A and B, while the leaf span would hold fields A, B, and C.
//!
//! In practice, these extra memory consumption used by these techniques should not matter for
//! modern systems, but may represent an unacceptable amount of memory usage on constrained systems
//! such as embedded platforms, etc.
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg), deny(broken_intra_doc_links))]

use metrics::{GaugeValue, Key, Label, Recorder, Unit};
use metrics_util::layers::Layer;

pub mod label_filter;
mod tracing_integration;

pub use label_filter::LabelFilter;
use tracing_integration::WithContext;
pub use tracing_integration::{Labels, MetricsLayer};

/// [`TracingContextLayer`] provides an implementation of a [`Layer`][metrics_util::layers::Layer]
/// for [`TracingContext`].
pub struct TracingContextLayer<F> {
    label_filter: F,
}

impl<F> TracingContextLayer<F> {
    /// Creates a new [`TracingContextLayer`].
    pub fn new(label_filter: F) -> Self {
        Self { label_filter }
    }
}

impl TracingContextLayer<label_filter::IncludeAll> {
    /// Creates a new [`TracingContextLayer`].
    pub fn all() -> Self {
        Self {
            label_filter: label_filter::IncludeAll,
        }
    }
}

impl<R, F> Layer<R> for TracingContextLayer<F>
where
    F: Clone,
{
    type Output = TracingContext<R, F>;

    fn layer(&self, inner: R) -> Self::Output {
        TracingContext {
            inner,
            label_filter: self.label_filter.clone(),
        }
    }
}

/// [`TracingContext`] is a [`metrics::Recorder`] that injects labels from [`tracing::Span`]s.
pub struct TracingContext<R, F> {
    inner: R,
    label_filter: F,
}

impl<R, F> TracingContext<R, F>
where
    F: LabelFilter,
{
    fn enhance_key(&self, key: &Key) -> Option<Key> {
        // Care is taken to only create a new key if absolutely necessary, which means avoiding
        // creating a new key if there are no tracing fields at all.
        //
        // Technically speaking, we would also want to avoid creating a new key if all the tracing
        // fields ended up being filtered out, but we don't have a great way of doing that without
        // first scanning the tracing fields to see if one of them would match, where the cost of
        // doing the iteration would likely exceed the cost of simply constructing the new key.
        tracing::dispatcher::get_default(|dispatch| {
            let current = dispatch.current_span();
            if let Some(id) = current.id() {
                // We're currently within a live tracing span, so see if we have an available
                // metrics context to grab any fields/labels out of.
                if let Some(ctx) = dispatch.downcast_ref::<WithContext>() {
                    let mut f = |new_labels: &[Label]| {
                        if !new_labels.is_empty() {
                            let (name, mut labels) = key.clone().into_parts();

                            let filtered_labels = new_labels
                                .iter()
                                .filter(|label| self.label_filter.should_include_label(label))
                                .cloned();
                            labels.extend(filtered_labels);

                            Some(Key::from_parts(name, labels))
                        } else {
                            None
                        }
                    };

                    // Pull in the span's fields/labels if they exist.
                    return ctx.with_labels(dispatch, id, &mut f);
                }
            }

            None
        })
    }
}

impl<R, F> Recorder for TracingContext<R, F>
where
    R: Recorder,
    F: LabelFilter,
{
    fn register_counter(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        self.inner.register_counter(key, unit, description)
    }

    fn register_gauge(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        self.inner.register_gauge(key, unit, description)
    }

    fn register_histogram(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        self.inner.register_histogram(key, unit, description)
    }

    fn increment_counter(&self, key: &Key, value: u64) {
        let new_key = self.enhance_key(key);
        let key = new_key.as_ref().unwrap_or(key);
        self.inner.increment_counter(key, value);
    }

    fn update_gauge(&self, key: &Key, value: GaugeValue) {
        let new_key = self.enhance_key(key);
        let key = new_key.as_ref().unwrap_or(key);
        self.inner.update_gauge(key, value);
    }

    fn record_histogram(&self, key: &Key, value: f64) {
        let new_key = self.enhance_key(key);
        let key = new_key.as_ref().unwrap_or(key);
        self.inner.record_histogram(key, value);
    }
}
