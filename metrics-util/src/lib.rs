//! Helper types and functions used within the metrics ecosystem.
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg), deny(broken_intra_doc_links))]
#![cfg_attr(not(feature = "std"), no_std)]
#![allow(incomplete_features)]
#![feature(generic_associated_types)]

#[cfg(feature = "std")]
mod bucket;
#[cfg(feature = "std")]
pub use bucket::AtomicBucket;

#[cfg(feature = "std")]
mod debugging;
#[cfg(feature = "std")]
pub use debugging::{DebugValue, DebuggingRecorder, Snapshotter};

#[cfg(feature = "std")]
mod handles;

mod quantile;
pub use quantile::{parse_quantiles, Quantile};

#[cfg(feature = "std")]
mod registry;
#[cfg(feature = "std")]
pub use registry::{Registry, StandardPrimitives};
//pub use registry::{Generation, Generational};

mod common;
pub use common::*;

mod key;
pub use key::CompositeKey;

mod kind;
pub use kind::{MetricKind, MetricKindMask};

mod histogram;
pub use histogram::Histogram;

#[cfg(feature = "std")]
mod summary;
#[cfg(feature = "std")]
pub use summary::Summary;

pub mod layers;

//#[cfg(feature = "std")]
//mod recency;
//#[cfg(feature = "std")]
//pub use recency::Recency;
