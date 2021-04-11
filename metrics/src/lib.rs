//! A lightweight metrics facade.
//!
//! The `metrics` crate provides a single metrics API that abstracts over the actual metrics
//! implementation.  Libraries can use the metrics API provided by this crate, and the consumer of
//! those libraries can choose the metrics implementation that is most suitable for its use case.
//!
//! # Overview
//! `metrics` exposes two main concepts: emitting a metric, and recording it.
//!
//! ## Metric types, or kinds
//! This crate supports three fundamental metric types, or kinds: counters, gauges, and histograms.
//!
//! ### Counters
//! A counter is a cumulative metric that represents a monotonically increasing value which can only
//! be increased or be reset to zero on restart. For example, you might use a counter to
//! represent the number of operations performed, or the number of errors that have occurred.
//!
//! Counters are unsigned 64-bit integers.
//!
//! If you have a value that goes up and down over time, consider using a gauge.
//!
//! ### Gauges
//! A gauge is a metric that can go up and down, arbitrarily, over time.
//!
//! Gauges are typically used for measured, external values, such as temperature, throughput, or
//! things like current memory usage.  Even if the value is monotonically increasing, but there is
//! no way to store the delta in order to properly figure out how much to increment by, then a gauge
//! might be a suitable choice.
//!
//! Gauges support two modes: incremental updates, or absolute updates.  This allows callers to use
//! them for external measurements -- where no delta can be computed -- as well as internal measurements.
//!
//! Gauges are floating-point 64-bit numbers.
//!
//! ### Histograms
//! A histogram stores an arbitrary number of observations of a specific measurement and provides
//! statistical analysis over the observed values.  Typically, measurements such as request latency
//! are recorded with histograms: a specific action that is repeated over and over which can have a
//! varying result each time.
//!
//! Histograms are used to explore the distribution of values, allowing a caller to understand the
//! modalities of the distribution, such as whether or not all values are grouped close together, or
//! spread evenly, or even whether or not there are multiple groupings or clusters.
//!
//! Colloquially, histograms are usually associated with percentiles, although by definition, they
//! specifically deal with bucketed or binned values: how many values fell within 0-10, how many
//! fell within 11-20, and so on and so forth.  Percentiles, commonly associated with "summaries",
//! deal with understanding how much of a distribution falls below or at a particular percentage of
//! that distribution: 50% of requests are slower than 500ms, 99% of requests are slower than
//! 2450ms, and so on and so forth.
//!
//! While we use the term "histogram" in `metrics`, we enforce no particular usage of true
//! histograms or summaries.  The choice of output is based entirely on the exporter being used to
//! ship your metric data out of your application.  For example, if you're using
//! [metrics-exporter-prometheus], Prometheus supports both histograms and summaries, and the
//! exporter can be configured to output our "histogram" data as either.  Other exporters may choose
//! to stick to using summaries, as is traditional, in order to generate percentile data.
//!
//! Histograms take floating-point 64-bit numbers.
//!
//! ## Emission
//! Metrics are emitted by utilizing the registration or emission macros.  There is a macro for
//! registering and emitting each fundamental metric type:
//! - [`register_counter!`], [`counter!`], and [`increment_counter!`] for counters
//! - [`register_gauge!`], [`gauge!`], [`increment_gauge!`], and [`decrement_gauge!`] for gauges
//! - [`register_histogram!`] and [`histogram!`] for histograms
//!
//! In order to register or emit a metric, you need a way to record these events, which is where
//! [`Recorder`] comes into play.
//!
//! ## Recording
//! The [`Recorder`] trait defines the interface between the registration/emission macros, and
//! exporters, which is how we refer to concrete implementations of [`Recorder`].  The trait defines
//! what the exporters are doing -- recording -- but ultimately exporters are sending data from your
//! application to somewhere else: whether it be a third-party service or logging via standard out.
//! It's "exporting" the metric data out of your application.
//!
//! Each metric type is usually reserved for a specific type of use case, whether it be tracking a
//! single value or allowing the summation of multiple values, and the respective macros elaborate
//! more on the usage and invariants provided by each.
//!
//! # Getting Started
//!
//! ## In libraries
//! Libraries need only include the `metrics` crate to emit metrics.  When an executable installs a
//! recorder, all included crates which emitting metrics will now emit their metrics to that record,
//! which allows library authors to seamless emit their own metrics without knowing or caring which
//! exporter implementation is chosen, or even if one is installed.
//!
//! In cases where no global recorder is installed, a "noop" recorder lives in its place, which has
//! an incredibly very low overhead: an atomic load and comparison.  Libraries can safely instrument
//! their code without fear of ruining baseline performance.
//!
//! By default, a "noop" recorder is present so that the macros can work even if no exporter has
//! been installed.  This recorder has extremely low overhead -- a relaxed load and conditional --
//! and so, practically speaking, the overhead when no exporter is installed is extremely low.  You
//! can safely instrument applications knowing that you won't pay a heavy performance cost even if
//! you're not shipping metrics.
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
//!     let delta = start.elapsed();
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
//! Executables, which themselves can emit their own metrics, are intended to install a global
//! recorder so that metrics can actually be recorded and exported somewhere.
//!
//! Initialization of the global recorder isn't required for macros to function, but any metrics
//! emitted before a global recorder is installed will not be recorded, so initialization and
//! installation of an exporter should happen as early as possible in the application lifecycle.
//!
//! ### Warning
//!
//! The metrics system may only be initialized once.
//!
//! For most use cases, you'll be using an off-the-shelf exporter implementation that hooks up to an
//! existing metrics collection system, or interacts with the existing systems/processes that you use.
//!
//! Out of the box, some exporter implementations are available for you to use:
//!
//! * [metrics-exporter-tcp] - outputs metrics to clients over TCP
//! * [metrics-exporter-prometheus] - serves a Prometheus scrape endpoint
//!
//! You can also implement your own recorder if a suitable one doesn't already exist.
//!
//! # Development
//!
//! The primary interface with `metrics` is through the [`Recorder`] trait, so we'll show examples
//! below of the trait and implementation notes.
//!
//! ## Implementing and installing a basic recorder
//!
//! Here's a basic example which writes metrics in text form via the `log` crate.
//!
//! ```rust
//! use log::info;
//! use metrics::{GaugeValue, Key, Recorder, Unit};
//! use metrics::SetRecorderError;
//!
//! struct LogRecorder;
//!
//! impl Recorder for LogRecorder {
//!     fn register_counter(&self, key: &Key, _unit: Option<Unit>, _description: Option<&'static str>) {}
//!
//!     fn register_gauge(&self, key: &Key, _unit: Option<Unit>, _description: Option<&'static str>) {}
//!
//!     fn register_histogram(&self, key: &Key, _unit: Option<Unit>, _description: Option<&'static str>) {}
//!
//!     fn increment_counter(&self, key: &Key, value: u64) {
//!         info!("counter '{}' -> {}", key, value);
//!     }
//!
//!     fn update_gauge(&self, key: &Key, value: GaugeValue) {
//!         info!("gauge '{}' -> {:?}", key, value);
//!     }
//!
//!     fn record_histogram(&self, key: &Key, value: f64) {
//!         info!("histogram '{}' -> {}", key, value);
//!     }
//! }
//!
//! // Recorders are installed by calling the [`set_recorder`] function.  Recorders should provide a
//! // function that wraps the creation and installation of the recorder:
//!
//! static RECORDER: LogRecorder = LogRecorder;
//!
//! pub fn init() -> Result<(), SetRecorderError> {
//!     metrics::set_recorder(&RECORDER)
//! }
//! # fn main() {}
//! ```
//! ## Keys
//!
//! All metrics are, in essence, the combination of a metric type and metric identifier, such as a
//! histogram called "response_latency".  You could conceivably have multiple metrics with the same
//! name, so long as they are of different types.
//!
//! As the types are enforced/limited by the [`Recorder`] trait itself, the remaining piece is the
//! identifier, which we handle by using [`Key`].
//!
//! [`Key`] itself is a wrapper for [`KeyData`], which holds not only the name of a metric, but
//! potentially holds labels for it as well.  Metric name and labels consist entirely of string
//! values.
//!
//! Internally, `metrics` uses a clone-on-write "smart pointer" for these values to optimize cases
//! where the values are static strings, which can provide significant performance benefits.  These
//! smart pointers can also hold owned `String` values, though, so users can mix and match static
//! strings and owned strings without issue.
//!
//! Two [`Key`] objects can be checked for equality and considered to point to the same metric if
//! they are equal.  Equality checks both the name of the key and the labels of a key.  Labels are
//! _not_ sorted prior to checking for equality, but insertion order is maintained, so any [`Key`]
//! constructed from the same set of labels in the same order should be equal.
//!
//! It is an implementation detail if a recorder wishes to do an deeper equality check that ignores
//! the order of labels, but practically speaking, metric emission, and thus labels, should be
//! fixed in ordering in nearly all cases, and so it typically is not a problem.
//!
//! ## Registration
//!
//! Recorders must handle the "registration" of a metric.
//!
//! In practice, registration solves two potential problems: providing metadata for a metric, and
//! creating an entry for a metric even though it has not been emitted yet.
//!
//! Callers may wish to provide a human-readable description of what the metric is, or provide the
//! units the metrics uses.  Additionally, users may wish to register their metrics so that they
//! show up in the output of the installed exporter even if the metrics have yet to be emitted.
//! This allows callers to ensure the metrics output is stable, or allows them to expose all of the
//! potential metrics a system has to offer, again, even if they have not all yet been emitted.
//!
//! As you can see from the trait, the registration methods treats the metadata as optional, and
//! the macros allow users to mix and match whichever fields they want to provide.
//!
//! When a metric is registered, the expectation is that it will show up in output with a default
//! value, so, for example, a counter should be initialized to zero, a histogram would have no
//! values, and so on.
//!
//! ## Emission
//!
//! Likewise, recorders must handle the emission of metrics as well.
//!
//! Comparatively speaking, emission is not too different from registration: you have access to the
//! same [`Key`] as well as the value being emitted.
//!
//! For recorders which temporarily buffer or hold on to values before exporting, a typical approach
//! would be to utilize atomic variables for the storage.  For counters and gauges, this can be done
//! simply by using types like [`AtomicU64`](std::sync::atomic::AtomicU64).  For histograms, this can be
//! slightly tricky as you must hold on to all of the distinct values.  In our helper crate,
//! [`metrics-util`][metrics-util], we've provided a type called [`AtomicBucket`][AtomicBucket].  For
//! exporters that will want to get all of the current values in a batch, while clearing the bucket so
//! that values aren't processed again, [AtomicBucket] provides a simple interface to do so, as well as
//! optimized performance on both the insertion and read side.
//!
//! Combined together, exporter authors can use [`Handle`][Handle], also from the `metrics-util`
//! crate, which provides a consolidated type for holding metric data.  These types, and many more
//! from the `metrics-util` crate, form the basis of typical exporter behavior and have been exposed
//! to help you quickly build a new exporter.
//!
//! ## Installing recorders
//!
//! In order to actually use an exporter, it must be installed as the "global" recorder.  This is a
//! static recorder that the registration and emission macros refer to behind-the-scenes.  `metrics`
//! provides a few methods to do so: [`set_recorder`], [`set_boxed_recorder`], and [`set_recorder_racy`].
//!
//! Primarily, you'll use [`set_boxed_recorder`] to pass a boxed version of the exporter to be
//! installed.  This is due to the fact that most exporters won't be able to be constructed
//! statically.  If you could construct your exporter statically, though, then you could instead
//! choose [`set_recorder`].
//!
//! Similarly, [`set_recorder_racy`] takes a static reference, but is also not thread safe, and
//! should only be used on platforms which do not support atomic operations, such as embedded
//! environments.
//!
//! As users of `metrics`, you'll typically see exporters provide methods to install themselves that
//! hide the nitty gritty details.  These methods will usually be aptly named, such as `install`.
//!
//! [metrics-exporter-tcp]: https://docs.rs/metrics-exporter-tcp
//! [metrics-exporter-prometheus]: https://docs.rs/metrics-exporter-prometheus
//! [metrics-util]: https://docs.rs/metrics-util
//! [AtomicBucket]: https://docs.rs/metrics-util/0.5.0/metrics_util/struct.AtomicBucket.html
//! [Handle]: https://docs.rs/metrics-util/0.5.0/metrics_util/enum.Handle.html
#![deny(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(docsrs, feature(doc_cfg), deny(broken_intra_doc_links))]

extern crate alloc;

mod common;
pub use self::common::*;

mod cow;

mod key;
pub use self::key::*;

mod label;
pub use self::label::*;

mod recorder;
pub use self::recorder::*;

/// Registers a counter.
///
/// Counters represent a single monotonic value, which means the value can only be incremented, not
/// decremented, and always starts out with an initial value of zero.
///
/// Metrics can be registered with an optional unit and description.  Whether or not the installed
/// recorder does anything with the description is implementation defined.  Labels can also be
/// specified when registering a metric.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::register_counter;
/// # use metrics::Unit;
/// # fn main() {
/// // A basic counter:
/// register_counter!("some_metric_name");
///
/// // Providing a unit for a counter:
/// register_counter!("some_metric_name", Unit::Bytes);
///
/// // Providing a description for a counter:
/// register_counter!("some_metric_name", "total number of bytes");
///
/// // Specifying labels:
/// register_counter!("some_metric_name", "service" => "http");
///
/// // We can combine the units, description, and labels arbitrarily:
/// register_counter!("some_metric_name", Unit::Bytes, "total number of bytes");
/// register_counter!("some_metric_name", Unit::Bytes, "service" => "http");
/// register_counter!("some_metric_name", "total number of bytes", "service" => "http");
///
/// // And all combined:
/// register_counter!("some_metric_name", Unit::Bytes, "number of woopsy daisies", "service" => "http");
///
/// // We can also pass labels by giving a vector or slice of key/value pairs.  In this scenario,
/// // a unit or description can still be passed in their respective positions:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// register_counter!("some_metric_name", &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// register_counter!(name);
///
/// register_counter!(format!("{}_via_format", "name"));
/// # }
/// ```
pub use metrics_macros::register_counter;

/// Registers a gauge.
///
/// Gauges represent a single value that can go up or down over time, and always starts out with an
/// initial value of zero.
///
/// Metrics can be registered with an optional unit and description.  Whether or not the installed
/// recorder does anything with the description is implementation defined.  Labels can also be
/// specified when registering a metric.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::register_gauge;
/// # use metrics::Unit;
/// # fn main() {
/// // A basic gauge:
/// register_gauge!("some_metric_name");
///
/// // Providing a unit for a gauge:
/// register_gauge!("some_metric_name", Unit::Bytes);
///
/// // Providing a description for a gauge:
/// register_gauge!("some_metric_name", "total number of bytes");
///
/// // Specifying labels:
/// register_gauge!("some_metric_name", "service" => "http");
///
/// // We can combine the units, description, and labels arbitrarily:
/// register_gauge!("some_metric_name", Unit::Bytes, "total number of bytes");
/// register_gauge!("some_metric_name", Unit::Bytes, "service" => "http");
/// register_gauge!("some_metric_name", "total number of bytes", "service" => "http");
///
/// // And all combined:
/// register_gauge!("some_metric_name", Unit::Bytes, "total number of bytes", "service" => "http");
///
/// // We can also pass labels by giving a vector or slice of key/value pairs.  In this scenario,
/// // a unit or description can still be passed in their respective positions:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// register_gauge!("some_metric_name", &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// register_gauge!(name);
///
/// register_gauge!(format!("{}_via_format", "name"));
/// # }
/// ```
pub use metrics_macros::register_gauge;

/// Records a histogram.
///
/// Histograms measure the distribution of values for a given set of measurements, and start with no
/// initial values.
///
/// Metrics can be registered with an optional unit and description.  Whether or not the installed
/// recorder does anything with the description is implementation defined.  Labels can also be
/// specified when registering a metric.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::register_histogram;
/// # use metrics::Unit;
/// # fn main() {
/// // A basic histogram:
/// register_histogram!("some_metric_name");
///
/// // Providing a unit for a histogram:
/// register_histogram!("some_metric_name", Unit::Nanoseconds);
///
/// // Providing a description for a histogram:
/// register_histogram!("some_metric_name", "request handler duration");
///
/// // Specifying labels:
/// register_histogram!("some_metric_name", "service" => "http");
///
/// // We can combine the units, description, and labels arbitrarily:
/// register_histogram!("some_metric_name", Unit::Nanoseconds, "request handler duration");
/// register_histogram!("some_metric_name", Unit::Nanoseconds, "service" => "http");
/// register_histogram!("some_metric_name", "request handler duration", "service" => "http");
///
/// // And all combined:
/// register_histogram!("some_metric_name", Unit::Nanoseconds, "request handler duration", "service" => "http");
///
/// // We can also pass labels by giving a vector or slice of key/value pairs.  In this scenario,
/// // a unit or description can still be passed in their respective positions:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// register_histogram!("some_metric_name", &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// register_histogram!(name);
///
/// register_histogram!(format!("{}_via_format", "name"));
/// # }
/// ```
pub use metrics_macros::register_histogram;

/// Increments a counter by one.
///
/// Counters represent a single monotonic value, which means the value can only be incremented, not
/// decremented, and always starts out with an initial value of zero.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::increment_counter;
/// # fn main() {
/// // A basic increment:
/// increment_counter!("some_metric_name");
///
/// // Specifying labels:
/// increment_counter!("some_metric_name", "service" => "http");
///
/// // We can also pass labels by giving a vector or slice of key/value pairs:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// increment_counter!("some_metric_name", &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// increment_counter!(name);
///
/// increment_counter!(format!("{}_via_format", "name"));
/// # }
/// ```
pub use metrics_macros::increment_counter;

/// Increments a counter.
///
/// Counters represent a single monotonic value, which means the value can only be incremented, not
/// decremented, and always starts out with an initial value of zero.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::counter;
/// # fn main() {
/// // A basic counter:
/// counter!("some_metric_name", 12);
///
/// // Specifying labels:
/// counter!("some_metric_name", 12, "service" => "http");
///
/// // We can also pass labels by giving a vector or slice of key/value pairs:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// counter!("some_metric_name", 12, &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// counter!(name, 12);
///
/// counter!(format!("{}_via_format", "name"), 12);
/// # }
/// ```
pub use metrics_macros::counter;

/// Updates a gauge.
///
/// Gauges represent a single value that can go up or down over time, and always starts out with an
/// initial value of zero.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::gauge;
/// # fn main() {
/// // A basic gauge:
/// gauge!("some_metric_name", 42.2222);
///
/// // Specifying labels:
/// gauge!("some_metric_name", 66.6666, "service" => "http");
///
/// // We can also pass labels by giving a vector or slice of key/value pairs:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// gauge!("some_metric_name", 42.42, &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// gauge!(name, 800.85);
///
/// gauge!(format!("{}_via_format", "name"), 3.14);
/// # }
/// ```
pub use metrics_macros::gauge;

/// Increments a gauge.
///
/// Gauges represent a single value that can go up or down over time, and always starts out with an
/// initial value of zero.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::increment_gauge;
/// # fn main() {
/// // A basic gauge:
/// increment_gauge!("some_metric_name", 42.2222);
///
/// // Specifying labels:
/// increment_gauge!("some_metric_name", 66.6666, "service" => "http");
///
/// // We can also pass labels by giving a vector or slice of key/value pairs:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// increment_gauge!("some_metric_name", 42.42, &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// increment_gauge!(name, 800.85);
///
/// increment_gauge!(format!("{}_via_format", "name"), 3.14);
/// # }
/// ```
pub use metrics_macros::increment_gauge;

/// Decrements a gauge.
///
/// Gauges represent a single value that can go up or down over time, and always starts out with an
/// initial value of zero.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::decrement_gauge;
/// # fn main() {
/// // A basic gauge:
/// decrement_gauge!("some_metric_name", 42.2222);
///
/// // Specifying labels:
/// decrement_gauge!("some_metric_name", 66.6666, "service" => "http");
///
/// // We can also pass labels by giving a vector or slice of key/value pairs:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// decrement_gauge!("some_metric_name", 42.42, &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// decrement_gauge!(name, 800.85);
///
/// decrement_gauge!(format!("{}_via_format", "name"), 3.14);
/// # }
/// ```
pub use metrics_macros::decrement_gauge;

/// Records a histogram.
///
/// Histograms measure the distribution of values for a given set of measurements, and start with no
/// initial values.
///
/// # Implicit conversions
/// Histograms are represented as `f64` values, but often come from another source, such as a time
/// measurement.  By default, `histogram!` will accept a `f64` directly or a
/// [`Duration`](std::time::Duration), which uses the floating-point number of seconds represents by
/// the duration.
///
/// External libraries and applications can create their own conversions by implementing the
/// [`IntoF64`] trait for their types, which is required for the value being passed to `histogram!`.
///
/// Metric names are shown below using string literals, but they can also be owned `String` values,
/// which includes using macros such as `format!` directly at the callsite. String literals are
/// preferred for performance where possible.
///
/// # Example
/// ```
/// # use metrics::histogram;
/// # use std::time::Duration;
/// # fn main() {
/// // A basic histogram:
/// histogram!("some_metric_name", 34.3);
///
/// // An implicit conversion from `Duration`:
/// let d = Duration::from_millis(17);
/// histogram!("some_metric_name", d);
///
/// // Specifying labels:
/// histogram!("some_metric_name", 38.0, "service" => "http");
///
/// // We can also pass labels by giving a vector or slice of key/value pairs:
/// let dynamic_val = "woo";
/// let labels = [("dynamic_key", format!("{}!", dynamic_val))];
/// histogram!("some_metric_name", 1337.5, &labels);
///
/// // As mentioned in the documentation, metric names also can be owned strings, including ones
/// // generated at the callsite via things like `format!`:
/// let name = String::from("some_owned_metric_name");
/// histogram!(name, 800.85);
///
/// histogram!(format!("{}_via_format", "name"), 3.14);
/// # }
/// ```
pub use metrics_macros::histogram;
