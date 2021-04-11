//! Use [`tracing::span!`] fields as [`metrics`] labels.
//!
//! The `metrics-tracing-context` crate provides tools to enable injecting the
//! contextual data maintained via `span!` macro from the [`tracing`] crate
//! into the metrics.
//!
//! # Use
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
//! # let mysubscriber = Registry::default();
//! let subscriber = mysubscriber.with(MetricsLayer::new());
//! tracing::subscriber::set_global_default(subscriber).unwrap();
//!
//! // Prepare metrics.
//! # let myrecorder = DebuggingRecorder::new();
//! let recorder = TracingContextLayer::all().layer(myrecorder);
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
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg), deny(broken_intra_doc_links))]

use metrics::{GaugeValue, Key, Label, Recorder, Unit};
use metrics_util::layers::Layer;
use tracing::Span;

pub mod label_filter;
mod tracing_integration;

pub use label_filter::LabelFilter;
pub use tracing_integration::{Labels, MetricsLayer, SpanExt};

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

/// [`TracingContext`] is a [`metrics::Recorder`] that injects labels from[`tracing::Span`]s.
pub struct TracingContext<R, F> {
    inner: R,
    label_filter: F,
}

impl<R, F> TracingContext<R, F>
where
    F: LabelFilter,
{
    fn enhance_labels(&self, labels: &mut Vec<Label>) {
        let span = Span::current();
        span.with_labels(|new_labels| {
            labels.extend(
                new_labels
                    .iter()
                    .filter(|&label| self.label_filter.should_include_label(label))
                    .cloned(),
            );
        });
    }

    fn enhance_key(&self, key: &Key) -> Key {
        let (name, mut labels) = key.clone().into_parts();
        self.enhance_labels(&mut labels);
        Key::from_parts(name, labels)
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
        let key = self.enhance_key(key);
        self.inner.increment_counter(&key, value);
    }

    fn update_gauge(&self, key: &Key, value: GaugeValue) {
        let key = self.enhance_key(key);
        self.inner.update_gauge(&key, value);
    }

    fn record_histogram(&self, key: &Key, value: f64) {
        let key = self.enhance_key(key);
        self.inner.record_histogram(&key, value);
    }
}
