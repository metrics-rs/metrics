//! A [`metrics`]-compatible exporter for sending metrics to a [DogStatsD][dsd]-compatible server.
//!
//! [dsd]: https://docs.datadoghq.com/developers/dogstatsd/
//!
//! # Usage
//!
//! Using the exporter is straightforward:
//!
//! ```no_run
//! # use metrics_exporter_dogstatsd::DogStatsDBuilder;
//! // First, create a builder.
//! //
//! // The builder can configure many aspects of the exporter, such as changing the listen address, adjusting how
//! // histograms will be reported, configuring sampling, and more.
//! let builder = DogStatsDBuilder::default();
//!
//! // At this point, any of the methods on `DogStatsDBuilder` can be called to configure the exporter, such as
//! // setting a non-default remote address, or configuring how histograms are sampled, and so on.
//!
//! // Normally, most users will want to "install" the exporter which sets it as the global recorder for all `metrics`
//! // calls, and creates the necessary background thread/task to flush the metrics to the remote DogStatsD server.
//! builder.install().expect("failed to install recorder/exporter");
//!
//! // For scenarios where you need access to the `Recorder` object, perhaps to wrap it in a layer stack, or something
//! // else, you can simply call `build` instead of `install`:
//! # let builder = DogStatsDBuilder::default();
//! let recorder = builder.build().expect("failed to build recorder");
//! ```
//!
//! # Features
//!
//! ## Client-side aggregation
//!
//! The exporter takes advantage of support in the DogStatsD protocol for aggregating metrics on the client-side, by
//! either utilizing multi-value payloads (for histograms, DSD v1.1) or aggregating points and specifying a timestamp
//! directly (for counters and gauges, DSD v1.3).
//!
//! This helps reduce load on the downstream DogStatsD server.
//!
//! ## Histogram sampling
//!
//! Histograms can be sampled at a configurable rate to limit the maximum per-histogram memory consumption and reduce
//! load on the downstream DogStatsD server. Reservoir sampling is used to ensure that the samples are statistically
//! representative over the overall population of values, even when the reservoir size is much smaller than the total
//! population size: we can hold 1,000 to 2,000 samples and still get a good representation when the number of input
//! values is in the millions.
//!
//! ## Smart reporting
//!
//! The exporter will "splay" the reporting of metrics over time, to smooth out the rate of payloads received by the
//! downstream DogStatsD server. This means that instead of reporting `N` metrics as fast as possibler every `M`
//! seconds, the exporter tries to ensure that over the period of `M` seconds, metrics are flushed at a rate of `M/N`.
//!
//! This is also designed to help reduce load on the downstream DogStatsD server to help avoid spiky resource
//! consumption and potentially dropped metrics.
//!
//! ## Full transport support for Unix domain sockets
//!
//! The exporter supports sending metrics to a DogStatsD server over all three major allowable transports: UDP, and Unix
//! domain sockets in either `SOCK_DGRAM` or `SOCK_STREAM` mode.
//!
//! `SOCK_STREAM` mode is roughly equivalent to TCP, but only available on the same host, and provides better
//! guarantees around message delivery in high-throughput scenarios.
//!
//! ## Telemetry
//!
//! The exporter captures its own internal telemetry around the number of active metrics, points flushed or dropped,
//! payloads/bytes sent, and so on. This telemetry can be emitted to the same downstream DogStatsD server as the
//! exporter itself.
//!
//! All internal telemetry is under the `datadog.dogstatsd.client` namespace, to align with the internal telemetry
//! emitted by official DogStatsD clients.
//!
//! # Missing
//!
//! ## Container ID detection
//!
//! We do not yet support container ID detection (DSD v1.2) which is used to help aid the downstream DogStatsD server in
//! enriching the metrics with additional metadata relevant to the host/application emitting the metrics.
//!
//! ## Asynchronous backend
//!
//! We do not yet support an asynchronous forwarder backend for flushing metrics.

#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::struct_excessive_bools)]
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg), deny(rustdoc::broken_intra_doc_links))]

mod builder;
pub use self::builder::{AggregationMode, BuildError, DogStatsDBuilder};

mod forwarder;
mod recorder;
pub use self::recorder::DogStatsDRecorder;

mod state;
mod storage;
mod telemetry;
pub(crate) mod util;
mod writer;
