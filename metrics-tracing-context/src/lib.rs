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
//! # use metrics_util::debugging::DebuggingRecorder;
//! # use tracing_subscriber::Registry;
//! use metrics_tracing_context::{MetricsLayer, TracingContextLayer};
//! use metrics_util::layers::Layer;
//! use tracing_subscriber::layer::SubscriberExt;
//!
//! // Prepare tracing.
//! # let my_subscriber = Registry::default();
//! let subscriber = my_subscriber.with(MetricsLayer::new());
//! tracing::subscriber::set_global_default(subscriber).unwrap();
//!
//! // Prepare metrics.
//! # let my_recorder = DebuggingRecorder::new();
//! let recorder = TracingContextLayer::all().layer(my_recorder);
//! metrics::set_global_recorder(recorder).unwrap();
//! ```
//!
//! Then emit some metrics within spans and see the labels being injected!
//!
//! ```rust
//! # use metrics_util::{debugging::DebuggingRecorder, layers::Layer};
//! # use tracing_subscriber::{layer::SubscriberExt, Registry};
//! # use metrics_tracing_context::{MetricsLayer, TracingContextLayer};
//! # let mysubscriber = Registry::default();
//! # let subscriber = mysubscriber.with(MetricsLayer::new());
//! # tracing::subscriber::set_global_default(subscriber).unwrap();
//! # let myrecorder = DebuggingRecorder::new();
//! # let recorder = TracingContextLayer::all().layer(myrecorder);
//! # metrics::set_global_recorder(recorder).unwrap();
//! use tracing::{span, Level};
//! use metrics::counter;
//!
//! let user = "ferris";
//! let span = span!(Level::TRACE, "login", user);
//! let _guard = span.enter();
//!
//! counter!("login_attempts", "service" => "login_service").increment(1);
//! ```
//!
//! The code above will emit a increment for a `login_attempts` counter with
//! the following labels:
//! - `service=login_service`
//! - `user=ferris`
//!
//! # Implementation
//!
//! The integration layer works by capturing all fields that are present when a span is created,
//! as well as fields recorded after the fact, and storing them as an extension to the span. If
//! a metric is emitted while a span is entered, any fields captured for that span will be added
//! to the metric as additional labels.
//!
//! Be aware that we recursively capture the fields of a span, including fields from
//! parent spans, and use them when generating metric labels. This means that if a metric is being
//! emitted in span B, which is a child of span A, and span A has field X, and span B has field Y,
//! then the metric labels will include both field X and Y. This applies regardless of how many
//! nested spans are currently entered.
//!
//! ## Duplicate span fields
//!
//! When span fields are captured, they are deduplicated such that only the most recent value is kept.
//! For merging parent span fields into the current span fields, the fields from the current span have
//! the highest priority. Additionally, when using [`Span::record`][tracing::Span::record] to add fields
//! to a span after it has been created, the same behavior applies. This means that recording a field
//! multiple times only keeps the most recently recorded value, including if a field was already present
//! from a parent span and is then recorded dynamically in the current span.
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
#![cfg_attr(docsrs, feature(doc_cfg), deny(rustdoc::broken_intra_doc_links))]

use metrics::{
    Counter, Gauge, Histogram, Key, KeyName, Label, Metadata, Recorder, SharedString, Unit,
};
use metrics_util::layers::Layer;

pub mod label_filter;
mod tracing_integration;

pub use label_filter::LabelFilter;
use tracing_integration::Map;
pub use tracing_integration::{Labels, MetricsLayer};

/// [`TracingContextLayer`] provides an implementation of a [`Layer`] for [`TracingContext`].
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
        Self { label_filter: label_filter::IncludeAll }
    }
}

impl TracingContextLayer<label_filter::Allowlist> {
    /// Creates a new [`TracingContextLayer`] that only allows labels contained
    /// in a predefined list.
    pub fn only_allow<I, S>(allowed: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self { label_filter: label_filter::Allowlist::new(allowed) }
    }
}

impl<R, F> Layer<R> for TracingContextLayer<F>
where
    F: Clone,
{
    type Output = TracingContext<R, F>;

    fn layer(&self, inner: R) -> Self::Output {
        TracingContext { inner, label_filter: self.label_filter.clone() }
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
            let id = current.id()?;
            let ctx = dispatch.downcast_ref::<MetricsLayer>()?;

            let mut f = |mut span_labels: Map| {
                (!span_labels.is_empty()).then(|| {
                    let (name, labels) = key.clone().into_parts();

                    // Filter only span labels
                    span_labels.retain(|key: &SharedString, value: &mut SharedString| {
                        let label = Label::new(key.clone(), value.clone());
                        self.label_filter.should_include_label(&name, &label)
                    });

                    // Overwrites labels from spans
                    span_labels.extend(labels.into_iter().map(Label::into_parts));

                    let labels = span_labels
                        .into_iter()
                        .map(|(key, value)| Label::new(key, value))
                        .collect::<Vec<_>>();

                    Key::from_parts(name, labels)
                })
            };

            // Pull in the span's fields/labels if they exist.
            ctx.with_labels(dispatch, id, &mut f)
        })
    }
}

impl<R, F> Recorder for TracingContext<R, F>
where
    R: Recorder,
    F: LabelFilter,
{
    fn describe_counter(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        self.inner.describe_counter(key_name, unit, description)
    }

    fn describe_gauge(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        self.inner.describe_gauge(key_name, unit, description)
    }

    fn describe_histogram(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        self.inner.describe_histogram(key_name, unit, description)
    }

    fn register_counter(&self, key: &Key, metadata: &Metadata<'_>) -> Counter {
        let new_key = self.enhance_key(key);
        let key = new_key.as_ref().unwrap_or(key);
        self.inner.register_counter(key, metadata)
    }

    fn register_gauge(&self, key: &Key, metadata: &Metadata<'_>) -> Gauge {
        let new_key = self.enhance_key(key);
        let key = new_key.as_ref().unwrap_or(key);
        self.inner.register_gauge(key, metadata)
    }

    fn register_histogram(&self, key: &Key, metadata: &Metadata<'_>) -> Histogram {
        let new_key = self.enhance_key(key);
        let key = new_key.as_ref().unwrap_or(key);
        self.inner.register_histogram(key, metadata)
    }
}
