//! A lightweight metrics facade.
//!
//! The `metrics` crate provides a single metrics API that abstracts over the actual metrics
//! implementation.  Libraries can use the metrics API provided by this crate, and the consumer of
//! those libraries can choose the metrics implementation that is most suitable for its use case.
//!
//! If no metrics implementation is selected, the facade falls back to a "noop" implementation that
//! ignores all metrics.  The overhead in this case is very small - an atomic load and comparison.
//!
//! # Use
//! The basic use of the facade crate is through the three metrics macros: [`counter!`], [`gauge!`],
//! and [`histogram!`].  These macros correspond to updating a counter, updating a gauge,
//! and updating a histogram.
//!
//! ## In libraries
//! Libraries should link only to the `metrics` crate, and use the provided macros to record
//! whatever metrics will be useful to downstream consumers.
//!
//! ### Examples
//!
//! ```rust
//! use metrics::{histogram, counter};
//!
//! # use std::time::Instant;
//! # pub fn run_query(_: &str) -> u64 { 42 }
//! pub fn process(query: &str) -> u64 {
//!     let start = Instant::now();
//!     let row_count = run_query(query);
//!     let delta = Instant::now() - start;
//!
//!     histogram!("process.query_time", delta);
//!     counter!("process.query_row_count", row_count);
//!
//!     row_count
//! }
//! # fn main() {}
//! ```
//!
//! ## In executables
//!
//! Executables should choose a metrics implementation and initialize it early in the runtime of
//! the program.  Metrics implementations will typically include a function to do this.  Any
//! metrics recordered before the implementation is initialized will be ignored.
//!
//! The executable itself may use the `metrics` crate to record metrics well.
//!
//! ### Warning
//!
//! The metrics system may only be initialized once.
//!
//! # Available metrics implementations
//!
//! * # Native recorder:
//!     * [metrics-exporter-tcp] - outputs metrics to clients over TCP
//!     * [metrics-exporter-prometheus] - serves a Prometheus scrape endpoint
//!
//! # Implementing a Recorder
//!
//! Recorders implement the [`Recorder`] trait.  Here's a basic example which writes the
//! metrics in text form via the `log` crate.
//!
//! ```rust
//! use log::info;
//! use metrics::{Key, Recorder, Unit};
//!
//! struct LogRecorder;
//!
//! impl Recorder for LogRecorder {
//!     fn register_counter(&self, key: Key, _unit: Option<Unit>, _description: Option<&'static str>) {}
//!
//!     fn register_gauge(&self, key: Key, _unit: Option<Unit>, _description: Option<&'static str>) {}
//!
//!     fn register_histogram(&self, key: Key, _unit: Option<Unit>, _description: Option<&'static str>) {}
//!
//!     fn increment_counter(&self, key: Key, value: u64) {
//!         info!("counter '{}' -> {}", key, value);
//!     }
//!
//!     fn update_gauge(&self, key: Key, value: f64) {
//!         info!("gauge '{}' -> {}", key, value);
//!     }
//!
//!     fn record_histogram(&self, key: Key, value: u64) {
//!         info!("histogram '{}' -> {}", key, value);
//!     }
//! }
//! # fn main() {}
//! ```
//!
//! Recorders are installed by calling the [`set_recorder`] function.  Recorders should provide a
//! function that wraps the creation and installation of the recorder:
//!
//! ```rust
//! # use metrics::{Key, Recorder, Unit};
//! # struct LogRecorder;
//! # impl Recorder for LogRecorder {
//! #     fn register_counter(&self, _key: Key, _unit: Option<Unit>, _description: Option<&'static str>) {}
//! #     fn register_gauge(&self, _key: Key, _unit: Option<Unit>, _description: Option<&'static str>) {}
//! #     fn register_histogram(&self, _key: Key, _unit: Option<Unit>, _description: Option<&'static str>) {}
//! #     fn increment_counter(&self, _key: Key, _value: u64) {}
//! #     fn update_gauge(&self, _key: Key, _value: f64) {}
//! #     fn record_histogram(&self, _key: Key, _value: u64) {}
//! # }
//! use metrics::SetRecorderError;
//!
//! static RECORDER: LogRecorder = LogRecorder;
//!
//! pub fn init() -> Result<(), SetRecorderError> {
//!     metrics::set_recorder(&RECORDER)
//! }
//! # fn main() {}
//! ```
//!
//! # Use with `std`
//!
//! `set_recorder` requires you to provide a `&'static Recorder`, which can be hard to
//! obtain if your recorder depends on some runtime configuration.  The `set_boxed_recorder`
//! function is available with the `std` Cargo feature.  It is identical to `set_recorder` except
//! that it takes a `Box<Recorder>` rather than a `&'static Recorder`:
//!
//! ```rust
//! # use metrics::{Key, Recorder, Unit};
//! # struct LogRecorder;
//! # impl Recorder for LogRecorder {
//! #     fn register_counter(&self, _key: Key, _unit: Option<Unit>, _description: Option<&'static str>) {}
//! #     fn register_gauge(&self, _key: Key, _unit: Option<Unit>, _description: Option<&'static str>) {}
//! #     fn register_histogram(&self, _key: Key, _unit: Option<Unit>, _description: Option<&'static str>) {}
//! #     fn increment_counter(&self, _key: Key, _value: u64) {}
//! #     fn update_gauge(&self, _key: Key, _value: f64) {}
//! #     fn record_histogram(&self, _key: Key, _value: u64) {}
//! # }
//! use metrics::SetRecorderError;
//!
//! # #[cfg(feature = "std")]
//! pub fn init() -> Result<(), SetRecorderError> {
//!     metrics::set_boxed_recorder(Box::new(LogRecorder))
//! }
//! # fn main() {}
//! ```
//!
//! [metrics-exporter-tcp]: https://docs.rs/metrics-exporter-tcp
//! [metrics-exporter-prometheus]: https://docs.rs/metrics-exporter-prometheus
#![deny(missing_docs)]
use proc_macro_hack::proc_macro_hack;

mod common;
pub use self::common::*;

mod key;
pub use self::key::*;

mod label;
pub use self::label::*;

mod recorder;
pub use self::recorder::*;

/// Registers a counter.
///
/// Counters represent a single value that can only be incremented over time, or reset to zero.
///
/// Metrics can be registered with an optional description.  Whether or not the installed recorder
/// does anything with the description is implementation defined.  Labels can also be specified
/// when registering a metric.
///
/// Counters, when registered, start at zero.
///
/// # Scoped versus unscoped
/// Metrics can be unscoped or scoped, where the scoping is derived by the current module the call
/// is taking place in.  This scope is used as a prefix to the provided metric name.
///
/// # Example
/// ```
/// # use metrics::register_counter;
/// # fn main() {
/// // A regular, unscoped counter:
/// register_counter!("some_metric_name");
///
/// // A scoped counter.  This inherits a scope derived by the current module:
/// register_counter!(<"some_metric_name">);
///
/// // Providing a description for a counter:
/// register_counter!("some_metric_name", "number of woopsy daisies");
///
/// // Specifying labels:
/// register_counter!("some_metric_name", "service" => "http");
///
/// // And all combined:
/// register_counter!("some_metric_name", "number of woopsy daisies", "service" => "http");
/// register_counter!(<"some_metric_name">, "number of woopsy daisies", "service" => "http");
///
/// // And just for an alternative form of passing labels:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// register_counter!("some_metric_name", &labels);
/// # }
/// ```
#[proc_macro_hack]
pub use metrics_macros::register_counter;

/// Registers a gauge.
///
/// Gauges represent a single value that can go up or down over time.
///
/// Metrics can be registered with an optional description.  Whether or not the installed recorder
/// does anything with the description is implementation defined.  Labels can also be specified
/// when registering a metric.
///
/// Gauges, when registered, start at zero.
///
/// # Scoped versus unscoped
/// Metrics can be unscoped or scoped, where the scoping is derived by the current module the call
/// is taking place in.  This scope is used as a prefix to the provided metric name.
///
/// # Example
/// ```
/// # use metrics::register_gauge;
/// # fn main() {
/// // A regular, unscoped gauge:
/// register_gauge!("some_metric_name");
///
/// // A scoped gauge.  This inherits a scope derived by the current module:
/// register_gauge!(<"some_metric_name">);
///
/// // Providing a description for a gauge:
/// register_gauge!("some_metric_name", "number of woopsy daisies");
///
/// // Specifying labels:
/// register_gauge!("some_metric_name", "service" => "http");
///
/// // And all combined:
/// register_gauge!("some_metric_name", "number of woopsy daisies", "service" => "http");
/// register_gauge!(<"some_metric_name">, "number of woopsy daisies", "service" => "http");
///
/// // And just for an alternative form of passing labels:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// register_gauge!("some_metric_name", &labels);
/// # }
/// ```
#[proc_macro_hack]
pub use metrics_macros::register_gauge;

/// Records a histogram.
///
/// Histograms measure the distribution of values for a given set of measurements.
///
/// Metrics can be registered with an optional description.  Whether or not the installed recorder
/// does anything with the description is implementation defined.  Labels can also be specified
/// when registering a metric.
///
/// Histograms, when registered, start at zero.
///
/// # Scoped versus unscoped
/// Metrics can be unscoped or scoped, where the scoping is derived by the current module the call
/// is taking place in.  This scope is used as a prefix to the provided metric name.
///
/// # Example
/// ```
/// # use metrics::register_histogram;
/// # fn main() {
/// // A regular, unscoped histogram:
/// register_histogram!("some_metric_name");
///
/// // A scoped histogram.  This inherits a scope derived by the current module:
/// register_histogram!(<"some_metric_name">);
///
/// // Providing a description for a histogram:
/// register_histogram!("some_metric_name", "number of woopsy daisies");
///
/// // Specifying labels:
/// register_histogram!("some_metric_name", "service" => "http");
///
/// // And all combined:
/// register_histogram!("some_metric_name", "number of woopsy daisies", "service" => "http");
/// register_histogram!(<"some_metric_name">, "number of woopsy daisies", "service" => "http");
///
/// // And just for an alternative form of passing labels:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// register_histogram!("some_metric_name", &labels);
/// # }
/// ```
#[proc_macro_hack]
pub use metrics_macros::register_histogram;

/// Increments a counter.
///
/// Counters represent a single value that can only be incremented over time, or reset to zero.
///
/// # Scoped versus unscoped
/// Metrics can be unscoped or scoped, where the scoping is derived by the current module the call
/// is taking place in.  This scope is used as a prefix to the provided metric name.
///
/// # Example
/// ```
/// # use metrics::increment;
/// # fn main() {
/// // A regular, unscoped increment:
/// increment!("some_metric_name");
///
/// // A scoped increment.  This inherits a scope derived by the current module:
/// increment!(<"some_metric_name">);
///
/// // Specifying labels:
/// increment!("some_metric_name", "service" => "http");
///
/// // And all combined:
/// increment!("some_metric_name", "service" => "http");
/// increment!(<"some_metric_name">, "service" => "http");
///
/// // And just for an alternative form of passing labels:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// increment!("some_metric_name", &labels);
/// # }
/// ```
#[proc_macro_hack]
pub use metrics_macros::increment;

/// Increments a counter.
///
/// Counters represent a single value that can only be incremented over time, or reset to zero.
///
/// # Scoped versus unscoped
/// Metrics can be unscoped or scoped, where the scoping is derived by the current module the call
/// is taking place in.  This scope is used as a prefix to the provided metric name.
///
/// # Example
/// ```
/// # use metrics::counter;
/// # fn main() {
/// // A regular, unscoped counter:
/// counter!("some_metric_name", 12);
///
/// // A scoped counter.  This inherits a scope derived by the current module:
/// counter!(<"some_metric_name">, 12);
///
/// // Specifying labels:
/// counter!("some_metric_name", 12, "service" => "http");
///
/// // And all combined:
/// counter!("some_metric_name", 12, "service" => "http");
/// counter!(<"some_metric_name">, 12, "service" => "http");
///
/// // And just for an alternative form of passing labels:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// counter!("some_metric_name", 12, &labels);
/// # }
/// ```
#[proc_macro_hack]
pub use metrics_macros::counter;

/// Updates a gauge.
///
/// Gauges represent a single value that can go up or down over time.
///
/// # Scoped versus unscoped
/// Metrics can be unscoped or scoped, where the scoping is derived by the current module the call
/// is taking place in.  This scope is used as a prefix to the provided metric name.
///
/// # Example
/// ```
/// # use metrics::gauge;
/// # fn main() {
/// // A regular, unscoped gauge:
/// gauge!("some_metric_name", 42.2222);
///
/// // A scoped gauge.  This inherits a scope derived by the current module:
/// gauge!(<"some_metric_name">, 33.3333);
///
/// // Specifying labels:
/// gauge!("some_metric_name", 66.6666, "service" => "http");
///
/// // And all combined:
/// gauge!("some_metric_name", 55.5555, "service" => "http");
/// gauge!(<"some_metric_name">, 11.1111, "service" => "http");
///
/// // And just for an alternative form of passing labels:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// gauge!("some_metric_name", 42.42, &labels);
/// # }
/// ```
#[proc_macro_hack]
pub use metrics_macros::gauge;

/// Records a histogram.
///
/// Histograms measure the distribution of values for a given set of measurements.
///
/// # Scoped versus unscoped
/// Metrics can be unscoped or scoped, where the scoping is derived by the current module the call
/// is taking place in.  This scope is used as a prefix to the provided metric name.
///
/// # Implicit conversions
/// Histograms are represented as `u64` values, but often come from another source, such as a time
/// measurement.  By default, `histogram!` will accept a `u64` directly or a
/// [`Duration`](std::time::Duration), which uses the nanoseconds total as the converted value.
///
/// External libraries and applications can create their own conversions by implementing the
/// [`IntoU64`] trait for their types, which is required for the value being passed to `histogram!`.
///
/// # Example
/// ```
/// # use metrics::histogram;
/// # use std::time::Duration;
/// # fn main() {
/// // A regular, unscoped histogram:
/// histogram!("some_metric_name", 34);
///
/// // An implicit conversion from `Duration`:
/// let d = Duration::from_millis(17);
/// histogram!("some_metric_name", d);
///
/// // A scoped histogram.  This inherits a scope derived by the current module:
/// histogram!(<"some_metric_name">, 38);
/// histogram!(<"some_metric_name">, d);
///
/// // Specifying labels:
/// histogram!("some_metric_name", 38, "service" => "http");
///
/// // And all combined:
/// histogram!("some_metric_name", d, "service" => "http");
/// histogram!(<"some_metric_name">, 57, "service" => "http");
///
/// // And just for an alternative form of passing labels:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// histogram!("some_metric_name", 1337, &labels);
/// # }
/// ```
#[proc_macro_hack]
pub use metrics_macros::histogram;
