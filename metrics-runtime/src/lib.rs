//! High-speed metrics collection library.
//!
//! `metrics-runtime` provides a generalized metrics collection library targeted at users who want
//! to log metrics at high volume and high speed.
//!
//! # Design
//!
//! The library follows a pattern of "senders" and a "receiver."
//!
//! Callers create a [`Receiver`], which acts as a registry for all metrics that flow through it.
//! It allows creating new sinks as well as controllers, both necessary to push in and pull out
//! metrics from the system.  It also manages background resources necessary for the registry to
//! operate.
//!
//! Once a [`Receiver`] is created, callers can either create a [`Sink`] for sending metrics, or a
//! [`Controller`] for getting metrics out.
//!
//! A [`Sink`] can be cheaply cloned, and offers convenience methods for getting the current time
//! as well as getting direct handles to a given metric.  This allows users to either work with the
//! fuller API exposed by [`Sink`] or to take a compositional approach and embed fields that
//! represent each particular metric to be sent.
//!
//! A [`Controller`] provides both a synchronous and asynchronous snapshotting interface, which is
//! [`metrics-core`][metrics_core] compatible for exporting.  This allows flexibility in
//! integration amongst traditional single-threaded or hand-rolled multi-threaded applications and
//! the emerging asynchronous Rust ecosystem.
//!
//! # Performance
//!
//! Users can expect to be able to send tens of millions of samples per second, with ingest
//! latencies at roughly 65-70ns at p50, and 250ns at p99.  Depending on the workload -- counters
//! vs histograms -- latencies may be even lower, as counters and gauges are markedly faster to
//! update than histograms.  Concurrent updates of the same metric will also cause natural
//! contention and lower the throughput/increase the latency of ingestion.
//!
//! # Metrics
//!
//! Counters, gauges, and histograms are supported, and follow the definitions outlined in
//! [`metrics-core`][metrics_core].
//!
//! Here's a simple example of creating a receiver and working with a sink:
//!
//! ```rust
//! # extern crate metrics_runtime;
//! use metrics_runtime::Receiver;
//! use std::{thread, time::Duration};
//! let receiver = Receiver::builder().build().expect("failed to create receiver");
//! let mut sink = receiver.get_sink();
//!
//! // We can update a counter.  Counters are monotonic, unsigned integers that start at 0 and
//! // increase over time.
//! sink.record_counter("widgets", 5);
//!
//! // We can update a gauge.  Gauges are signed, and hold on to the last value they were updated
//! // to, so you need to track the overall value on your own.
//! sink.record_gauge("red_balloons", 99);
//!
//! // We can update a timing histogram.  For timing, we're using the built-in `Sink::now` method
//! // which utilizes a high-speed internal clock.  This method returns the time in nanoseconds, so
//! // we get great resolution, but giving the time in nanoseconds isn't required!  If you want to
//! // send it in another unit, that's fine, but just pay attention to that fact when viewing and
//! // using those metrics once exported.  We also support passing `Instant` values -- both `start`
//! // and `end` need to be the same type, though! -- and we'll take the nanosecond output of that.
//! let start = sink.now();
//! thread::sleep(Duration::from_millis(10));
//! let end = sink.now();
//! sink.record_timing("db.queries.select_products_ns", start, end);
//!
//! // Finally, we can update a value histogram.  Technically speaking, value histograms aren't
//! // fundamentally different from timing histograms.  If you use a timing histogram, we do the
//! // math for you of getting the time difference, but other than that, identical under the hood.
//! let row_count = 46;
//! sink.record_value("db.queries.select_products_num_rows", row_count);
//! ```
//!
//! # Scopes
//!
//! Metrics can be scoped, not unlike loggers, at the [`Sink`] level.  This allows sinks to easily
//! nest themselves without callers ever needing to care about where they're located.
//!
//! This feature is a simpler approach to tagging: while not as semantically rich, it provides the
//! level of detail necessary to distinguish a single metric between multiple callsites.
//!
//! For example, after getting a [`Sink`] from the [`Receiver`], we can easily nest ourselves under
//! the root scope and then send some metrics:
//!
//! ```rust
//! # extern crate metrics_runtime;
//! use metrics_runtime::Receiver;
//! let receiver = Receiver::builder().build().expect("failed to create receiver");
//!
//! // This sink has no scope aka the root scope.  The metric will just end up as "widgets".
//! let mut root_sink = receiver.get_sink();
//! root_sink.record_counter("widgets", 42);
//!
//! // This sink is under the "secret" scope.  Since we derived ourselves from the root scope,
//! // we're not nested under anything, but our metric name will end up being "secret.widgets".
//! let mut scoped_sink = root_sink.scoped("secret");
//! scoped_sink.record_counter("widgets", 42);
//!
//! // This sink is under the "supersecret" scope, but we're also nested!  The metric name for this
//! // sample will end up being "secret.supersecret.widget".
//! let mut scoped_sink_two = scoped_sink.scoped("supersecret");
//! scoped_sink_two.record_counter("widgets", 42);
//!
//! // Sinks retain their scope even when cloned, so the metric name will be the same as above.
//! let mut cloned_sink = scoped_sink_two.clone();
//! cloned_sink.record_counter("widgets", 42);
//!
//! // This sink will be nested two levels deeper than its parent by using a slightly different
//! // input scope: scope can be a single string, or multiple strings, which is interpreted as
//! // nesting N levels deep.
//! //
//! // This metric name will end up being "super.secret.ultra.special.widgets".
//! let mut scoped_sink_three = scoped_sink.scoped(&["super", "secret", "ultra", "special"]);
//! scoped_sink_two.record_counter("widgets", 42);
//! ```
//!
//! # Labels
//!
//! On top of scope support, metrics can also have labels. If scopes are for organizing metrics in
//! a hierarchy, then labels are for differentiating the same metric being emitted from multiple
//! sources.
//!
//! This is most easily demonstrated with an example:
//!
//! ```rust
//! # extern crate metrics_runtime;
//! # fn run_query(_: &str) -> u64 { 42 }
//! use metrics_runtime::Receiver;
//! let receiver = Receiver::builder().build().expect("failed to create receiver");
//!
//! let mut sink = receiver.get_sink();
//!
//! // We might have a function that interacts with a database and returns the number of rows it
//! // touched in doing so.
//! fn process_query(query: &str) -> u64 {
//!     run_query(query)
//! }
//!
//! // We might call this function multiple times, but hitting different tables.
//! let rows_a = process_query("UPDATE posts SET public = 1 WHERE public = 0");
//! let rows_b = process_query("UPDATE comments SET public = 1 WHERE public = 0");
//!
//! // Now, we want to track a metric that shows how many rows are updated overall, so the metric
//! // name should be the same no matter which table we update, but we'd also like to be able to
//! // differentiate by table, too!
//! sink.record_value_with_labels("db.rows_updated", rows_a, &[("table", "posts")]);
//! sink.record_value_with_labels("db.rows_updated", rows_b, &[("table", "comments")]);
//!
//! // If you want to send a specific set of labels with every metric from this sink, you can also
//! // add default labels.  This action is additive, so you can call it multiple times to build up
//! // the set of labels sent with metrics, and labels are inherited when creating a scoped sink or
//! // cloning an existing sink, which allows label usage to either supplement scopes or to
//! // potentially replace them entirely.
//! sink.add_default_labels(&[("database", "primary")]);
//! # fn main() {}
//! ```
//!
//! As shown in the example, labels allow a user to submit values to the underlying metric name,
//! while also differentiating between unique situations, whatever the facet that the user decides
//! to utilize.
//!
//! Naturally, these methods can be slightly cumbersome and visually detracting, in which case
//! you can utilize the proxy metric types -- [`Counter`], [`Gauge`], and [`Histogram`] -- and
//! create them with labels ahead of time.  These types, by their nature, are bound to a specific
//! metric, which encompasses name, scope, and labels, and so extra labels cannot be passed when
//! actually updating them.
//!
//! # Snapshots
//!
//! Naturally, we need a way to get the metrics out of the system, which is where snapshots come
//! into play.  By utilizing a [`Controller`], we can take a snapshot of the current metrics in the
//! registry, and then output them to any desired system/interface by utilizing
//! [`Observer`](metrics_core::Observer).  A number of pre-baked observers (which only concern
//! themselves with formatting the data) and exporters (which take the formatted data and either
//! serve it up, such as exposing an HTTP endpoint, or write it somewhere, like stdout) are
//! available, some of which are exposed by this crate.
//!
//! Let's take an example of writing out our metrics in a yaml-like format, writing them via
//! `log!`:
//! ```rust
//! # extern crate metrics_runtime;
//! use metrics_runtime::{
//!     Receiver, observers::TextBuilder, exporters::LogExporter,
//! };
//! use log::Level;
//! use std::{thread, time::Duration};
//! let receiver = Receiver::builder().build().expect("failed to create receiver");
//! let mut sink = receiver.get_sink();
//!
//! // We can update a counter.  Counters are monotonic, unsigned integers that start at 0 and
//! // increase over time.
//! // Take some measurements, similar to what we had in other examples:
//! sink.record_counter("widgets", 5);
//! sink.record_gauge("red_balloons", 99);
//!
//! let start = sink.now();
//! thread::sleep(Duration::from_millis(10));
//! let end = sink.now();
//! sink.record_timing("db.queries.select_products_ns", start, end);
//! sink.record_timing("db.gizmo_query", start, end);
//!
//! let num_rows = 46;
//! sink.record_value("db.queries.select_products_num_rows", num_rows);
//!
//! // Now create our exporter/observer configuration, and wire it up.
//! let exporter = LogExporter::new(
//!     receiver.get_controller(),
//!     TextBuilder::new(),
//!     Level::Info,
//!     Duration::from_secs(5),
//! );
//!
//! // This exporter will now run every 5 seconds, taking a snapshot, rendering it, and writing it
//! // via `log!` at the informational level. This particular exporter is running directly on the
//! // current thread, and not on a background thread.
//! //
//! // exporter.run();
//! ```
//! Most exporters have the ability to run on the current thread or to be converted into a future
//! which can be spawned on any Tokio-compatible runtime.
//!
//! # Facade
//!
//! `metrics-runtime` is `metrics` compatible, and can be installed as the global metrics facade:
//! ```
//! # #[macro_use] extern crate metrics;
//! extern crate metrics_runtime;
//! use metrics_runtime::Receiver;
//!
//! Receiver::builder()
//!     .build()
//!     .expect("failed to create receiver")
//!     .install();
//!
//! counter!("items_processed", 42);
//! ```
//!
//! [metrics_core]: https://docs.rs/metrics-core
//! [`Observer`]: https://docs.rs/metrics-core/0.3.1/metrics_core/trait.Observer.html
#![deny(missing_docs)]
#![warn(unused_extern_crates)]
mod builder;
mod common;
mod config;
mod control;
pub mod data;
mod helper;
mod receiver;
mod registry;
mod sink;

#[cfg(any(feature = "metrics-exporter-log", feature = "metrics-exporter-http"))]
pub mod exporters;

#[cfg(any(
    feature = "metrics-observer-text",
    feature = "metrics-observer-prometheus"
))]
pub mod observers;

pub use self::{
    builder::{Builder, BuilderError},
    common::{Delta, Scope},
    control::Controller,
    receiver::Receiver,
    sink::{AsScoped, Sink, SinkError},
};
