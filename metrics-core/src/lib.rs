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
use std::borrow::Cow;
use std::time::Duration;
use futures::future::Future;

/// An optimized metric key.
///
/// As some metrics might be sent at high frequency, it makes no sense to constantly allocate and
/// reallocate owned [`String`]s when a static [`str`] would suffice.  As we don't want to limit
/// callers, though, we opt to use a copy-on-write pointer -- [`Cow`] -- to allow callers
/// flexiblity in how and what they pass.
pub type Key = Cow<'static, str>;

/// A value which can be converted into a nanosecond representation.
///
/// This trait allows us to interchangably accept raw integer time values, ones already in
/// nanoseconds, as well as the more conventional [`Duration`] which is a result of getting the
/// difference between two [`Instant`](std::time::Instant)s.
pub trait AsNanoseconds {
    fn as_nanos(&self) -> u64;
}

impl AsNanoseconds for u64 {
    fn as_nanos(&self) -> u64 { *self }
}

impl AsNanoseconds for Duration {
    fn as_nanos(&self) -> u64 {
        self.as_nanos() as u64
    }
}

/// A value that records metrics.
pub trait Recorder {
    /// Records a counter.
    ///
    /// From the perspective of an recorder, a counter and gauge are essentially identical, insofar
    /// as they are both a single value tied to a key.  From the perspective of a collector,
    /// counters and gauges usually have slightly different modes of operation.
    ///
    /// For the sake of flexibility on the exporter side, both are provided.
    fn record_counter(&mut self, key: Key, value: u64);

    /// Records a gauge.
    ///
    /// From the perspective of a recorder, a counter and gauge are essentially identical, insofar
    /// as they are both a single value tied to a key.  From the perspective of a collector,
    /// counters and gauges usually have slightly different modes of operation.
    ///
    /// For the sake of flexibility on the exporter side, both are provided.
    fn record_gauge(&mut self, key: Key, value: i64);

    /// Records a histogram.
    ///
    /// Recorders are expected to tally their own histogram views, so this will be called with all
    /// of the underlying observed values, and callers will need to process them accordingly.
    ///
    /// There is no guarantee that this method will not be called multiple times for the same key.
    fn record_histogram(&mut self, key: Key, values: &[u64]);
}

/// A value that holds a point-in-time view of collected metrics.
pub trait Snapshot {
    /// Records the snapshot to the given recorder.
    fn record<R: Recorder>(&self, recorder: &mut R);
}

/// A value that can provide on-demand snapshots.
pub trait SnapshotProvider {
    type Snapshot: Snapshot;
    type SnapshotError;

    /// Gets a snapshot.
    fn get_snapshot(&self) -> Result<Self::Snapshot, Self::SnapshotError>;
}

/// A value that can provide on-demand snapshots asynchronously.
pub trait AsyncSnapshotProvider {
    type Snapshot: Snapshot;
    type SnapshotError;
    type SnapshotFuture: Future<Item = Self::Snapshot, Error = Self::SnapshotError>;

    /// Gets a snapshot asynchronously.
    fn get_snapshot_async(&self) -> Self::SnapshotFuture;
}
