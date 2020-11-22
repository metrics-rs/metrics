//! Records metrics in the Prometheus exposition format.
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg), deny(broken_intra_doc_links))]
mod common;
pub use self::common::Matcher;

mod distribution;

mod builder;
pub use self::builder::PrometheusBuilder;

mod recorder;
pub use self::recorder::{PrometheusHandle, PrometheusRecorder};
