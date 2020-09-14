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
//! # use metrics_util::{layers::Layer, DebugValue, DebuggingRecorder, MetricKind, Snapshotter};
//! # use tracing_subscriber::{layer::SubscriberExt, Registry};
//! use metrics_tracing_context::{MetricsLayer, TracingContextLayer};
//!
//! // Prepare tracing.
//! # let mysubscriber = Registry::default();
//! let subscriber = mysubscriber.with(MetricsLayer::new());
//! tracing::subscriber::set_global_default(subscriber).unwrap();
//!
//! // Prepare metrics.
//! # let myrecorder = DebuggingRecorder::new();
//! let recorder = TracingContextLayer.layer(myrecorder);
//! metrics::set_boxed_recorder(Box::new(recorder)).unwrap();
//! ```
//!
//! Then emit some metrics within spans and see the labels being injected!
//!
//! ```rust
//! # use metrics_util::{layers::Layer, DebugValue, DebuggingRecorder, MetricKind, Snapshotter};
//! # use tracing_subscriber::{layer::SubscriberExt, Registry};
//! # use metrics_tracing_context::{MetricsLayer, TracingContextLayer};
//! # let mysubscriber = Registry::default();
//! # let subscriber = mysubscriber.with(MetricsLayer::new());
//! # tracing::subscriber::set_global_default(subscriber).unwrap();
//! # let myrecorder = DebuggingRecorder::new();
//! # let recorder = TracingContextLayer.layer(myrecorder);
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

use metrics::{Key, KeyData, Label, Recorder};
use metrics_util::layers::Layer;
use tracing::Span;

mod tracing_integration;

pub use tracing_integration::{MetricsLayer, SpanExt};

/// [`TracingContextLayer`] provides an implementation of a [`metrics::Layer`]
/// for [`TracingContext`].
pub struct TracingContextLayer;

impl<R> Layer<R> for TracingContextLayer {
    type Output = TracingContext<R>;

    fn layer(&self, inner: R) -> Self::Output {
        TracingContext { inner }
    }
}

/// [`TracingContext`] is a [`metrics::Recorder`] that injects labels from the
/// [`tracing::span`]s.
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

    fn enhance_key(&self, key: Key) -> Key {
        let (name, mut labels) = key.into_owned().into_parts();
        self.enhance_labels(&mut labels);
        KeyData::from_name_and_labels(name, labels).into()
    }
}

impl<R: Recorder> Recorder for TracingContext<R> {
    fn register_counter(&self, key: Key, description: Option<&'static str>) {
        self.inner.register_counter(key, description)
    }

    fn register_gauge(&self, key: Key, description: Option<&'static str>) {
        self.inner.register_gauge(key, description)
    }

    fn register_histogram(&self, key: Key, description: Option<&'static str>) {
        self.inner.register_histogram(key, description)
    }

    fn increment_counter(&self, key: Key, value: u64) {
        let key = self.enhance_key(key);
        self.inner.increment_counter(key, value);
    }

    fn update_gauge(&self, key: Key, value: f64) {
        let key = self.enhance_key(key);
        self.inner.update_gauge(key, value);
    }

    fn record_histogram(&self, key: Key, value: u64) {
        let key = self.enhance_key(key);
        self.inner.record_histogram(key, value);
    }
}
