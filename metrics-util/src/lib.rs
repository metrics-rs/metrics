//! Helper types and functions used within the metrics ecosystem.
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg), deny(rustdoc::broken_intra_doc_links))]

#[cfg(feature = "handles")]
mod bucket;
#[cfg(feature = "handles")]
#[cfg_attr(docsrs, doc(cfg(feature = "handles")))]
pub use bucket::AtomicBucket;

#[cfg(feature = "debugging")]
#[cfg_attr(docsrs, doc(cfg(feature = "debugging")))]
pub mod debugging;

#[cfg(feature = "handles")]
mod handles;

mod quantile;
pub use quantile::{parse_quantiles, Quantile};

#[cfg(feature = "registry")]
#[cfg_attr(docsrs, doc(cfg(feature = "registry")))]
pub mod registry;

mod common;
pub use common::*;

mod key;
pub use key::CompositeKey;

mod kind;
pub use kind::{MetricKind, MetricKindMask};

mod histogram;
pub use histogram::Histogram;

mod recoverable;
pub use recoverable::RecoverableRecorder;

#[cfg(feature = "summary")]
mod summary;
#[cfg(feature = "summary")]
#[cfg_attr(docsrs, doc(cfg(feature = "summary")))]
pub use summary::Summary;

pub mod layers;

#[cfg(test)]
mod test_util;
