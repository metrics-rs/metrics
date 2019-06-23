//! Helper types and functions used within the metrics ecosystem.
#![deny(missing_docs)]
mod bucket;
pub use bucket::AtomicBucket;

mod streaming;
pub use streaming::StreamingIntegers;

mod quantile;
pub use quantile::{parse_quantiles, Quantile};
