//! High-speed metrics collection library.
//!
//! `metrics` provides a generalized metrics collection library targeted at users who want to log
//! metrics at high volume and high speed.
//!
//! # Design
//!
//! The library follows a pattern of "senders" and a "receiver."
//!
//! Callers create a [`Receiver`], which acts as a contained unit: metric registration,
//! aggregation, and summarization.  The [`Receiver`] is intended to be spawned onto a dedicated
//! background thread.
//!
//! Once a [`Receiver`] is created, callers can either create a [`Sink`] for sending metrics, or a
//! [`Controller`] for getting metrics out.
//!
//! A [`Sink`] can be cheaply cloned and does not require a mutable reference to send metrics, so
//! callers have increased flexibility in usage and control over whether or not to clone sinks,
//! share references, etc.
//!
//! A [`Controller`] provides both a synchronous and asynchronous snapshotting interface, which is
//! [`metrics-core`][metrics_core] compatible for exporting.  This allows flexibility in
//! integration amongst traditional single-threaded or hand-rolled multi-threaded applications and
//! the emerging asynchronous Rust ecosystem.
//!
//! # Performance
//!
//! Being based on [`crossbeam-channel`][crossbeam_channel] allows us to process close to ten
//! million metrics per second using a single core, with average ingest latencies of around 100ns.
//!
//! # Metrics
//!
//! Counters, gauges, and histograms are supported, and follow the definitions outlined in
//! [`metrics-core`][metrics_core].
//!
//! Here's a simple example of creating a receiver and working with a sink:
//!
//! ```
//! # extern crate metrics;
//! use metrics::Receiver;
//! use std::{thread, time::Duration};
//! let receiver = Receiver::builder().build();
//! let sink = receiver.get_sink();
//!
//! // We can update a counter.  Counters are monotonic, unsigned integers that start at 0 and
//! // increase over time.
//! sink.record_count("widgets", 5);
//!
//! // We can update a gauge.  Gauges are signed, and hold on to the last value they were updated
//! // to, so you need to track the overall value on your own.
//! sink.record_gauge("red_balloons", 99);
//!
//! // We can update a timing histogram.  For timing, we're using the built-in `Sink::now` method
//! // which utilizes a high-speed internal clock.  This method returns the time in nanoseconds, so
//! // we get great resolution, but giving the time in nanoseconds isn't required!  If you want to
//! // send it in another unit, that's fine, but just pay attention to that fact when viewing and
//! // using those metrics once exported.
//! let start = sink.now();
//! thread::sleep(Duration::from_millis(10));
//! let end = sink.now();
//! sink.record_timing("db.gizmo_query", start, end);
//!
//! // Finally, we can update a value histogram.  Technically speaking, value histograms aren't
//! // fundamentally different from timing histograms.  If you use a timing histogram, we do the
//! // math for you of getting the time difference, and we make sure the metric name has the right
//! // unit suffix so you can tell it's measuring time, but other than that, nearly identical!
//! let buf_size = 4096;
//! sink.record_value("buf_size", buf_size);
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
//! ```
//! # extern crate metrics;
//! use metrics::Receiver;
//! let receiver = Receiver::builder().build();
//!
//! // This sink has no scope aka the root scope.  The metric will just end up as "widgets".
//! let root_sink = receiver.get_sink();
//! root_sink.record_count("widgets", 42);
//!
//! // This sink is under the "secret" scope.  Since we derived ourselves from the root scope,
//! // we're not nested under anything, but our metric name will end up being "secret.widgets".
//! let scoped_sink = root_sink.scoped("secret");
//! scoped_sink.record_count("widgets", 42);
//!
//! // This sink is under the "supersecret" scope, but we're also nested!  The metric name for this
//! // sample will end up being "secret.supersecret.widget".
//! let scoped_sink_two = scoped_sink.scoped("supersecret");
//! scoped_sink_two.record_count("widgets", 42);
//!
//! // Sinks retain their scope even when cloned, so the metric name will be the same as above.
//! let cloned_sink = scoped_sink_two.clone();
//! cloned_sink.record_count("widgets", 42);
//!
//! // This sink will be nested two levels deeper than its parent by using a slightly different
//! // input scope: scope can be a single string, or multiple strings, which is interpreted as
//! // nesting N levels deep.
//! //
//! // This metric name will end up being "super.secret.ultra.special.widgets".
//! let scoped_sink_three = scoped_sink.scoped(&["super", "secret", "ultra", "special"]);
//! scoped_sink_two.record_count("widgets", 42);
//! ```
//!
//! [crossbeam_channel]: https://docs.rs/crossbeam-channel
//! [metrics_core]: https://docs.rs/metrics-core
mod configuration;
mod control;
mod data;
mod helper;
mod receiver;
mod scopes;
mod sink;

#[cfg(any(
    feature = "metrics-exporter-log",
    feature = "metrics-exporter-http"
))]
pub mod exporters;

#[cfg(any(
    feature = "metrics-recorder-text",
    feature = "metrics-recorder-prometheus"
))]
pub mod recorders;

pub use self::{
    configuration::Configuration,
    control::{Controller, SnapshotError},
    data::histogram::HistogramSnapshot,
    receiver::Receiver,
    sink::{AsScoped, Sink, SinkError},
};

pub mod snapshot {
    pub use super::data::snapshot::{Snapshot, TypedMeasurement};
}
