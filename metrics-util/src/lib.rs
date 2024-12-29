//! Helper types and functions used within the metrics ecosystem.
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg), deny(rustdoc::broken_intra_doc_links))]

#[cfg(feature = "debugging")]
#[cfg_attr(docsrs, doc(cfg(feature = "debugging")))]
pub mod debugging;

mod quantile;
pub use self::quantile::{parse_quantiles, Quantile};

#[cfg(feature = "registry")]
#[cfg_attr(docsrs, doc(cfg(feature = "registry")))]
pub mod registry;

#[cfg(feature = "storage")]
#[cfg_attr(docsrs, doc(cfg(feature = "storage")))]
pub mod storage;

mod common;
pub use common::*;

mod key;
pub use key::CompositeKey;

mod kind;
pub use kind::{MetricKind, MetricKindMask};

mod recoverable;
pub use recoverable::RecoverableRecorder;

pub mod layers;

#[cfg(test)]
mod test_util;
