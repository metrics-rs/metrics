//! Helper types and functions used within the metrics ecosystem.
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg), deny(broken_intra_doc_links))]

mod bucket;
pub use bucket::AtomicBucket;

mod debugging;
pub use debugging::{DebugValue, DebuggingRecorder, Snapshotter};

mod handles;

mod quantile;
pub use quantile::{parse_quantiles, Quantile};

mod registry;
pub use registry::{Registry, StandardPrimitives};

mod common;
pub use common::*;

mod key;
pub use key::CompositeKey;

mod kind;
pub use kind::{MetricKind, MetricKindMask};

mod histogram;
pub use histogram::Histogram;

mod summary;
pub use summary::Summary;

pub mod layers;

pub mod recency;
