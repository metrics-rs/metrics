//! High-speed metrics collection library.
//!
//! hotmic provides a generalized metrics collection library targeted at users who want to log
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
//! From a [`Receiver`], callers can create a [`Sink`], which allows registering facets -- or
//! interests -- in a given metric, along with sending the metrics themselves.  All metrics need to
//! be pre-registered, in essence, with the receiver, which allows us to know which aspects of a
//! metric to track: count, value, or percentile.
//!
//! A [`Sink`] can be cheaply cloned and does not require a mutable reference to send metrics, and
//! so callers have great flexibility in being able to control their resource consumption when it
//! comes to sinks. [`Receiver`] also allows configuring the capacity of the underlying channels to
//! finely tune resource consumption.
//!
//! Being based on [`crossbeam-channel`] allows us to process close to fifteen million metrics per
//! second on a single core, with very low ingest latencies: 100ns on average at full throughput.
//!
//! # Metrics
//!
//! hotmic supports counters, gauges, and histograms.
//!
//! A counter is a single value that can be updated with deltas to increase or decrease the value.
//! This would be your typical "messages sent" or "database queries executed" style of metric,
//! where the value changes over time.
//!
//! A gauge is also a single value but does not support delta updates.  When a gauge is set, the
//! value sent becomes _the_ value of the gauge.  Gauges can be useful for metrics that measure a
//! point-in-time value, such as "connected clients" or "running queries".  While those metrics
//! could also be represented by a count, gauges can be simpler in cases where you're already
//! computing and storing the value, and simply want to expose it in your metrics.
//!
//! A histogram tracks the distribution of values: how many values were between 0-5, between 6-10,
//! etc.  This is the canonical way to measure latency: the time spent running a piece of code or
//! servicing an operation.  By keeping track of the individual measurements, we can better see how
//! many are slow, fast, average, and in what proportions.
//!
//! ```
//! # extern crate hotmic;
//! use hotmic::Receiver;
//! use std::{thread, time::Duration};
//! let receiver = Receiver::builder().build();
//! let sink = receiver.get_sink();
//!
//! // We can update a counter.  Counters are signed, and can be updated either with a delta, or
//! // can be incremented and decremented with the [`Sink::increment`] and [`Sink::decrement`].
//! sink.update_count("widgets", 5);
//! sink.update_count("widgets", -3);
//! sink.increment("widgets");
//! sink.decrement("widgets");
//!
//! // We can update a gauge.  Gauges are unsigned, and hold on to the last value they were updated
//! // to, so you need to track the overall value on your own.
//! sink.update_gauge("red_balloons", 99);
//!
//! // We can update a timing histogram.  For timing, you also must measure the start and end
//! // time using the built-in `Clock` exposed by the sink.  The receiver internally converts the
//! // raw values to calculate the actual wall clock time (in nanoseconds) on your behalf, so you
//! // can't just pass in any old number.. otherwise you'll get erroneous measurements!
//! let start = sink.clock().start();
//! thread::sleep(Duration::from_millis(10));
//! let end = sink.clock().end();
//! let rows = 42;
//!
//! // This would just set the timing:
//! sink.update_timing("db.gizmo_query", start, end);
//!
//! // This would set the timing and also let you provide a customized count value.  Being able to
//! // specify a count is handy when tracking things like the time it took to execute a database
//! // query, along with how many rows that query returned:
//! sink.update_timing_with_count("db.gizmo_query", start, end, rows);
//!
//! // Finally, we can update a value histogram.  Technically speaking, value histograms aren't
//! // fundamentally different from timing histograms.  If you use a timing histogram, we do the
//! // math for you of getting the time difference, and we make sure the metric name has the right
//! // unit suffix so you can tell it's measuring time, but other than that, nearly identical!
//! let buf_size = 4096;
//! sink.update_value("buf_size", buf_size);
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
//! An important thing to note is: registered metrics are only good for the scope they were
//! registered at.  If you create a scoped [`Sink`], you must register, or reregister, the metrics
//! you will be sending to it.
//!
//! For example, after getting a [`Sink`] from the [`Receiver`], we can easily nest ourselves under
//! the root scope and then send some metrics:
//!
//! ```
//! # extern crate hotmic;
//! use hotmic::Receiver;
//! let receiver = Receiver::builder().build();
//!
//! // This sink has no scope aka the root scope.  The metric will just end up as "widgets".
//! let root_sink = receiver.get_sink();
//! root_sink.update_count("widgets", 42);
//!
//! // This sink is under the "secret" scope.  Since we derived ourselves from the root scope,
//! // we're not nested under anything, but our metric name will end up being "secret.widgets".
//! let scoped_sink = root_sink.scoped("secret");
//! scoped_sink.update_count("widgets", 42);
//!
//! // This sink is under the "supersecret" scope, but we're also nested!  The metric name for this
//! // sample will end up being "secret.supersecret.widget".
//! let scoped_sink_two = scoped_sink.scoped("supersecret");
//! scoped_sink_two.update_count("widgets", 42);
//!
//! // Sinks retain their scope even when cloned, so the metric name will be the same as above.
//! let cloned_sink = scoped_sink_two.clone();
//! cloned_sink.update_count("widgets", 42);
//!
//! // This sink will be nested two levels deeper than its parent by using a slightly different
//! // input scope: scope can be a single string, or multiple strings, which is interpreted as
//! // nesting N levels deep.
//! //
//! // This metric name will end up being "super.secret.ultra.special.widgets".
//! let scoped_sink_three = scoped_sink.scoped(&["super", "secret", "ultra", "special"]);
//! scoped_sink_two.update_count("widgets", 42);
//! ```
mod configuration;
mod control;
mod data;
mod helper;
mod receiver;
mod scopes;
mod sink;

pub use self::{
    configuration::Configuration,
    control::{Controller, SnapshotError},
    receiver::Receiver,
    sink::{Sink, SinkError},
};

pub mod snapshot {
    pub use super::data::snapshot::{Snapshot, TypedMeasurement};
}
