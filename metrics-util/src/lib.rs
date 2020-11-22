//! Helper types and functions used within the metrics ecosystem.
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg), deny(broken_intra_doc_links))]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
mod bucket;
#[cfg(feature = "std")]
pub use bucket::AtomicBucket;

#[cfg(feature = "std")]
mod debugging;
#[cfg(feature = "std")]
pub use debugging::{DebugValue, DebuggingRecorder, MetricKind, Snapshotter};

#[cfg(feature = "std")]
mod handle;
#[cfg(feature = "std")]
pub use handle::Handle;

mod quantile;
pub use quantile::{parse_quantiles, Quantile};

#[cfg(feature = "std")]
mod registry;
#[cfg(feature = "std")]
pub use registry::{Generation, Registry};

mod key;
pub use key::CompositeKey;

mod histogram;
pub use histogram::Histogram;

pub mod layers;
