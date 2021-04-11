//! Layers are composable helpers that can be "layered" on top of an existing `Recorder` to enhance
//! or alter its behavior as desired, without having to change the recorder implementation itself.
//!
//! As well, [`Stack`] can be used to easily compose multiple layers together and provides a
//! convenience method for installing it as the global recorder, providing a smooth transition from
//! working directly with installing exporters to installing stacks.
//!
//! Here's an example of a layer that filters out all metrics that start with a specific string:
//!
//! ```rust
//! # use metrics::{GaugeValue, Key, Recorder, Unit};
//! # use metrics_util::DebuggingRecorder;
//! # use metrics_util::layers::{Layer, Stack, PrefixLayer};
//! // A simple layer that denies any metrics that have "stairway" or "heaven" in their name.
//! #[derive(Default)]
//! pub struct StairwayDeny<R>(pub(crate) R);
//!
//! impl<R> StairwayDeny<R> {
//!     fn is_invalid_key(&self, key: &Key) -> bool {
//!         for part in key.name().parts() {
//!             if part.contains("stairway") || part.contains("heaven") {
//!                 return true
//!             }
//!         }
//!         false
//!     }
//! }
//!
//! impl<R: Recorder> Recorder for StairwayDeny<R> {
//!    fn register_counter(&self, key: Key, unit: Option<Unit>, description: Option<&'static str>) {
//!        if self.is_invalid_key(&key) {
//!            return;
//!        }
//!        self.0.register_counter(key, unit, description)
//!    }
//!
//!    fn register_gauge(&self, key: Key, unit: Option<Unit>, description: Option<&'static str>) {
//!        if self.is_invalid_key(&key) {
//!            return;
//!        }
//!        self.0.register_gauge(key, unit, description)
//!    }
//!
//!    fn register_histogram(&self, key: Key, unit: Option<Unit>, description: Option<&'static str>) {
//!        if self.is_invalid_key(&key) {
//!            return;
//!        }
//!        self.0.register_histogram(key, unit, description)
//!    }
//!
//!    fn increment_counter(&self, key: Key, value: u64) {
//!        if self.is_invalid_key(&key) {
//!            return;
//!        }
//!        self.0.increment_counter(key, value);
//!    }
//!
//!    fn update_gauge(&self, key: Key, value: GaugeValue) {
//!        if self.is_invalid_key(&key) {
//!            return;
//!        }
//!        self.0.update_gauge(key, value);
//!    }
//!
//!    fn record_histogram(&self, key: Key, value: f64) {
//!        if self.is_invalid_key(&key) {
//!            return;
//!        }
//!        self.0.record_histogram(key, value);
//!    }
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
//! let recorder = DebuggingRecorder::new();
//! let layer = StairwayDenyLayer::default();
//! let layered = layer.layer(recorder);
//! metrics::set_boxed_recorder(Box::new(layered)).expect("failed to install recorder");
//!
//! # metrics::clear_recorder();
//!
//! // Working with layers directly is a bit cumbersome, though, so let's use a `Stack`.
//! let stack = Stack::new(DebuggingRecorder::new());
//! stack.push(StairwayDenyLayer::default())
//!     .install()
//!     .expect("failed to install stack");
//!
//! # metrics::clear_recorder();
//!
//! // `Stack` makes it easy to chain layers together, as well.
//! let stack = Stack::new(DebuggingRecorder::new());
//! stack.push(PrefixLayer::new("app_name"))
//!     .push(StairwayDenyLayer::default())
//!     .install()
//!     .expect("failed to install stack");
//! # }
//! ```
use metrics::{GaugeValue, Key, Recorder, Unit};

#[cfg(feature = "std")]
use metrics::SetRecorderError;

#[cfg(feature = "layer-filter")]
mod filter;
#[cfg(feature = "layer-filter")]
pub use filter::{Filter, FilterLayer};

mod prefix;
pub use prefix::{Prefix, PrefixLayer};

mod fanout;
pub use fanout::{Fanout, FanoutBuilder};

#[cfg(feature = "layer-absolute")]
mod absolute;
#[cfg(feature = "layer-absolute")]
pub use absolute::{Absolute, AbsoluteLayer};

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

#[cfg(feature = "std")]
impl<R: Recorder + 'static> Stack<R> {
    /// Installs this stack as the global recorder.
    ///
    /// An error will be returned if there's an issue with installing the stack as the global recorder.
    pub fn install(self) -> Result<(), SetRecorderError> {
        metrics::set_boxed_recorder(Box::new(self))
    }
}

impl<R: Recorder> Recorder for Stack<R> {
    fn register_counter(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        self.inner.register_counter(key, unit, description);
    }

    fn register_gauge(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        self.inner.register_gauge(key, unit, description);
    }

    fn register_histogram(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        self.inner.register_histogram(key, unit, description);
    }

    fn increment_counter(&self, key: &Key, value: u64) {
        self.inner.increment_counter(key, value);
    }

    fn update_gauge(&self, key: &Key, value: GaugeValue) {
        self.inner.update_gauge(key, value);
    }

    fn record_histogram(&self, key: &Key, value: f64) {
        self.inner.record_histogram(key, value);
    }
}
