//! Foundational traits for interoperable metrics libraries in Rust.
//!
//! # Common Ground
//! Most libraries, under the hood, are all based around a core set of data types: counters,
//! gauges, and histograms.  While the API surface may differ, the underlying data is the same.
//!
//! # Metric Types
//!
//! ## Counters
//! Counters represent a single value that can only ever be incremented over time, or reset to
//! zero.
//!
//! Counters are useful for tracking things like operations completed, or errors raised, where
//! the value naturally begins at zero when a process or service is started or restarted.
//!
//! ## Gauges
//! Gauges represent a single value that can go up _or_ down over time.
//!
//! Gauges are useful for tracking things like the current number of connected users, or a stock
//! price, or the temperature outside.
//!
//! ## Histograms
//! Histograms measure the distribution of values for a given set of measurements.
//!
//! Histograms are generally used to derive statistics about a particular measurement from an
//! operation or event that happens over and over, such as the duration of a request, or number of
//! rows returned by a particular database query.
//!
//! Histograms allow you to answer questions of these measurements, such as:
//! - "What were the fastest and slowest requests in this window?"
//! - "What is the slowest request we've seen out of 90% of the requests measured? 99%?"
//!
//! Histograms are a convenient way to measure behavior not only at the median, but at the edges of
//! normal operating behavior.
#![deny(missing_docs)]
use std::{borrow::Cow, time::Duration};

mod key;
mod label;
mod recorder;

pub use self::key::*;
pub use self::label::*;
pub use self::recorder::*;

/// An allocation-optimized string.
///
/// We specify `ScopedString` to attempt to get the best of both worlds: flexibility to provide a
/// static or dynamic (owned) string, while retaining the performance benefits of being able to
/// take ownership of owned strings and borrows of completely static strings.
pub type ScopedString = Cow<'static, str>;

/// Opaque identifier for a metric.
#[derive(Default)]
pub struct Identifier(usize);

/// Used to do a nanosecond conversion.
///
/// This trait allows us to interchangably accept raw integer time values, ones already in
/// nanoseconds, as well as the more conventional [`Duration`] which is a result of getting the
/// difference between two [`Instant`](std::time::Instant)s.
pub trait AsNanoseconds {
    /// Performs the conversion.
    fn as_nanos(&self) -> u64;
}

impl AsNanoseconds for u64 {
    fn as_nanos(&self) -> u64 {
        *self
    }
}

impl AsNanoseconds for Duration {
    fn as_nanos(&self) -> u64 {
        self.as_nanos() as u64
    }
}

/// Helper macro for generating a set of labels.
///
/// While a `Label` can be generated manually, most users will tend towards the key => value format
/// commonly used for defining hashes/maps in many programming languages.  This macro allows users
/// to do the exact same thing in calls that depend on [`metrics_core::IntoLabels`].
///
/// # Examples
/// ```rust
/// # #[macro_use] extern crate metrics_core;
/// # use metrics_core::IntoLabels;
/// fn takes_labels<L: IntoLabels>(name: &str, labels: L) {
///     println!("name: {} labels: {:?}", name, labels.into_labels());
/// }
///
/// takes_labels("requests_processed", labels!("request_type" => "admin"));
/// ```
#[macro_export]
macro_rules! labels {
    (@ { $($out:expr),* $(,)* } $(,)*) => {
        std::vec![ $($out),* ]
    };

    (@ { } $k:expr => $v:expr, $($rest:tt)*) => {
        $crate::labels!(@ { $crate::Label::new($k, $v) } $($rest)*)
    };

    (@ { $($out:expr),+ } $k:expr => $v:expr, $($rest:tt)*) => {
        $crate::labels!(@ { $($out),+, $crate::Label::new($k, $v) } $($rest)*)
    };

    ($($args:tt)*) => {
        $crate::labels!(@ { } $($args)*, )
    };
}
