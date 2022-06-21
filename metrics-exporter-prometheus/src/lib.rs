//! A [`metrics`]-compatible exporter for sending metrics to Prometheus.
//!
//! ## Basics
//!
//! `metrics-exporter-prometheus` is a [`metrics`]-compatible exporter for either exposing an HTTP
//! endpoint that can be scraped by Prometheus, or that can push metrics to a Prometheus push
//! gateway.
//!
//! ## High-level features
//!
//! - scrape endpoint support
//! - push gateway support
//! - IP-based allowlist for scrape endpoint
//! - ability to push histograms as either aggregated summaries or aggregated histograms, with
//!   configurable quantiles/buckets
//! - ability to control bucket configuration on a per-metric basis
//! - configurable global labels (applied to all metrics, overridden by metric's own labels if present)
//!
//! ## Behavior
//!
//! In general, interacting with the exporter should look and feel like interacting with any other
//! implementation of a Prometheus scrape endpoint or push gateway implementation, but there are
//! some small caveats around metric naming.
//!
//! We strive to match both the Prometheus [data model] and follow the [exposition format]
//! specification, but due to the decoupled nature of [`metrics`][metrics], the exporter makes some
//! specific trade-offs when ensuring compliance with the specification when it comes to metric
//! names and label keys.  Below is a matrix of scenarios where the exporter will modify a metric
//! name or label key:
//!
//! - metric name starts with, or contains, an invalid character: **replace character with
//!   underscore**
//! - label key starts with, or contains, an invalid character: **replace character with
//!   underscore**
//! - label key starts with two underscores: **add additional underscore** (three underscores total)
//!
//! This behavior may be confusing at first since [`metrics`][metrics] itself allows any valid UTF-8
//! string for a metric name or label, but there is no way to report to the user that a metric name
//! or label key is invalid only when using the Prometheus exporter, so we must cope with these
//! situations by replacing invalid characters at runtime.
//!
//! ## Usage
//!
//! Using the exporter is straightforward:
//!
//! ```ignore
//! // First, create a builder.
//! //
//! // The builder can configure many aspects of the exporter, such as changing the
//! // listen address, adjusting how histograms will be reported, changing how long
//! // metrics can be idle before being removed, and more.
//! let builder = PrometheusBuilder::new();
//!
//! // Normally, most users will want to "install" the exporter which sets it as the
//! // global recorder for all `metrics` calls, and installs either an HTTP listener
//! // when running as a scrape endpoint, or a simple asynchronous task which pushes
//! // to the configured push gateway on the given interval.
//! //
//! // If you're already inside a Tokio runtime, this will spawn a task for the
//! // exporter on that runtime, and otherwise, a new background thread will be
//! // spawned which a Tokio single-threaded runtime is launched on to, where we then
//! // finally launch the exporter:
//! builder.install().expect("failed to install recorder/exporter");
//!
//! // Maybe you already have an HTTP endpoint that you want to expose a metrics
//! // endpoint on.. no problem!  You can build the recorder and install it, and get
//! // back a handle that can be used to generate the Prometheus scrape output on
//! // demand:
//! let handle = builder.install_recorder().expect("failed to install recorder");
//!
//! // Maybe you have a more complicated setup and want to be handed back the recorder
//! // object and a future that can run the HTTP listener / push gateway so you can
//! // install/spawn them in a specific way.. also not a problem!
//! //
//! // As this is a more advanced method, it _must_ be called from within an existing
//! // Tokio runtime when the exporter is running in HTTP listener/scrape endpoint mode.
//! let (recorder, exporter) = builder.build().expect("failed to build recorder/exporter");
//!
//! // Finally, maybe you literally only want to build the recorder and nothing else,
//! // and we've got you covered there, too:
//! let recorder = builder.build_recorder().expect("failed to build recorder");
//! ```
//!
//! ## Features
//!
//! Two main feature flags control which modes that exporter can run in:
//! - **`http-listener`**: allows running the exporter as a scrape endpoint (_enabled by default_)
//! - **`push-gateway`**: allows running the exporter in push gateway mode (_enabled by default_)
//!
//! Neither of these flags are required to create, or install, only a recorder.  However, in order
//! to create or build an exporter, at least one of these feature flags must be enabled.  Builder
//! methods that require certain feature flags will be documented as such.
//!
//! [metrics]: https://docs.rs/metrics/latest/metrics/
//! [data model]: https://prometheus.io/docs/concepts/data_model/
//! [exposition format]: https://prometheus.io/docs/instrumenting/exposition_formats/#text-based-format
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::module_name_repetitions)]
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg), deny(rustdoc::broken_intra_doc_links))]
mod common;
pub use self::common::{BuildError, Matcher};

mod distribution;
pub use distribution::{Distribution, DistributionBuilder};

mod builder;
pub use self::builder::PrometheusBuilder;

pub mod formatting;
mod recorder;

mod registry;

pub use self::recorder::{PrometheusHandle, PrometheusRecorder};
