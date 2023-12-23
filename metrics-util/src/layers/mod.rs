//! Layers are composable helpers that can be "layered" on top of an existing `Recorder` to enhance
//! or alter its behavior as desired, without having to change the recorder implementation itself.
//!
//! As well, [`Stack`] can be used to easily compose multiple layers together and provides a
//! convenience method for installing it as the global recorder, providing a smooth transition from
//! working directly with installing exporters to installing stacks.
//!
//! Here's an example of a layer that filters out all metrics that start with a specific string:
//!
//! ```no_run
//! # use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};
//! # use metrics::NoopRecorder as BasicRecorder;
//! # use metrics_util::layers::{Layer, Stack, PrefixLayer};
//! // A simple layer that denies any metrics that have "stairway" or "heaven" in their name.
//! #[derive(Default)]
//! pub struct StairwayDeny<R>(pub(crate) R);
//!
//! impl<R> StairwayDeny<R> {
//!     fn is_invalid_key(&self, key: &str) -> bool {
//!         key.contains("stairway") || key.contains("heaven")
//!     }
//! }
//!
//! impl<R: Recorder> Recorder for StairwayDeny<R> {
//!     fn describe_counter(
//!         &self,
//!         key_name: KeyName,
//!         unit: Option<Unit>,
//!         description: SharedString,
//!     ) {
//!         if self.is_invalid_key(key_name.as_str()) {
//!             return;
//!         }
//!         self.0.describe_counter(key_name, unit, description)
//!     }
//!
//!     fn describe_gauge(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
//!         if self.is_invalid_key(key_name.as_str()) {
//!             return;
//!         }
//!         self.0.describe_gauge(key_name, unit, description)
//!     }
//!
//!     fn describe_histogram(
//!         &self,
//!         key_name: KeyName,
//!         unit: Option<Unit>,
//!         description: SharedString,
//!     ) {
//!         if self.is_invalid_key(key_name.as_str()) {
//!             return;
//!         }
//!         self.0.describe_histogram(key_name, unit, description)
//!     }
//!
//!     fn register_counter(&self, key: &Key, metadata: &Metadata<'_>) -> Counter {
//!         if self.is_invalid_key(key.name()) {
//!             return Counter::noop();
//!         }
//!         self.0.register_counter(key, metadata)
//!     }
//!
//!     fn register_gauge(&self, key: &Key, metadata: &Metadata<'_>) -> Gauge {
//!         if self.is_invalid_key(key.name()) {
//!             return Gauge::noop();
//!         }
//!         self.0.register_gauge(key, metadata)
//!     }
//!
//!     fn register_histogram(&self, key: &Key, metadata: &Metadata<'_>) -> Histogram {
//!         if self.is_invalid_key(key.name()) {
//!             return Histogram::noop();
//!         }
//!         self.0.register_histogram(key, metadata)
//!     }
//! }
//!
//! #[derive(Default)]
//! pub struct StairwayDenyLayer;
//!
//! impl<R> Layer<R> for StairwayDenyLayer {
//!     type Output = StairwayDeny<R>;
//!
//!     fn layer(&self, inner: R) -> Self::Output {
//!         StairwayDeny(inner)
//!     }
//! }
//!
//! // Now you can construct an instance of it to use it.  The layer will be wrapped around
//! // our base recorder, which is a debugging recorder also supplied by `metrics_util`.
//! # fn main() {
//! let recorder = BasicRecorder;
//! let layer = StairwayDenyLayer::default();
//! let layered = layer.layer(recorder);
//! metrics::set_global_recorder(layered).expect("failed to install recorder");
//!
//! // Working with layers directly is a bit cumbersome, though, so let's use a `Stack`.
//! let stack = Stack::new(BasicRecorder);
//! stack.push(StairwayDenyLayer::default()).install().expect("failed to install stack");
//!
//! // `Stack` makes it easy to chain layers together, as well.
//! let stack = Stack::new(BasicRecorder);
//! stack
//!     .push(PrefixLayer::new("app_name"))
//!     .push(StairwayDenyLayer::default())
//!     .install()
//!     .expect("failed to install stack");
//! # }
//! ```
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};

use metrics::SetRecorderError;

mod fanout;
pub use fanout::{Fanout, FanoutBuilder};

#[cfg(feature = "layer-filter")]
mod filter;
#[cfg(feature = "layer-filter")]
pub use filter::{Filter, FilterLayer};

mod prefix;
pub use prefix::{Prefix, PrefixLayer};

#[cfg(feature = "layer-router")]
mod router;
#[cfg(feature = "layer-router")]
pub use router::{Router, RouterBuilder};

/// Decorates an object by wrapping it within another type.
pub trait Layer<R> {
    /// The output type after wrapping.
    type Output;

    /// Wraps `inner` based on this layer.
    fn layer(&self, inner: R) -> Self::Output;
}

/// Builder for composing layers together in a top-down/inside-out order.
pub struct Stack<R> {
    inner: R,
}

impl<R> Stack<R> {
    /// Creates a new `Stack` around the given object.
    pub fn new(inner: R) -> Self {
        Stack { inner }
    }

    /// Pushes the given layer on to the stack, wrapping the existing stack.
    pub fn push<L: Layer<R>>(self, layer: L) -> Stack<L::Output> {
        Stack::new(layer.layer(self.inner))
    }
}

impl<R: Recorder + 'static> Stack<R> {
    /// Installs this stack as the global recorder.
    ///
    /// An error will be returned if there's an issue with installing the stack as the global recorder.
    pub fn install(self) -> Result<(), SetRecorderError<Self>> {
        metrics::set_global_recorder(self)
    }
}

impl<R: Recorder> Recorder for Stack<R> {
    fn describe_counter(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        self.inner.describe_counter(key_name, unit, description);
    }

    fn describe_gauge(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        self.inner.describe_gauge(key_name, unit, description);
    }

    fn describe_histogram(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        self.inner.describe_histogram(key_name, unit, description);
    }

    fn register_counter(&self, key: &Key, metadata: &Metadata<'_>) -> Counter {
        self.inner.register_counter(key, metadata)
    }

    fn register_gauge(&self, key: &Key, metadata: &Metadata<'_>) -> Gauge {
        self.inner.register_gauge(key, metadata)
    }

    fn register_histogram(&self, key: &Key, metadata: &Metadata<'_>) -> Histogram {
        self.inner.register_histogram(key, metadata)
    }
}
