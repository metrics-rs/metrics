use std::{
    borrow::Cow,
    fmt::{self, Display},
};

pub mod counter;
pub mod gauge;
pub mod histogram;
pub mod snapshot;

pub(crate) use self::{counter::Counter, gauge::Gauge, histogram::Histogram, snapshot::Snapshot};

pub type MetricKey = Cow<'static, str>;

/// A measurement.
///
/// Samples are the decoupled way of submitting data into the sink.
#[derive(Debug)]
pub(crate) enum Sample {
    /// A counter delta.
    ///
    /// The value is added directly to the existing counter, and so negative deltas will decrease
    /// the counter, and positive deltas will increase the counter.
    Count(ScopedKey, u64),

    /// A single value, also known as a gauge.
    ///
    /// Values operate in last-write-wins mode.
    ///
    /// Values themselves cannot be incremented or decremented, so you must hold them externally
    /// before sending them.
    Gauge(ScopedKey, i64),

    /// A timed sample.
    ///
    /// Includes the start and end times.
    TimingHistogram(ScopedKey, u64, u64),

    /// A single value measured over time.
    ///
    /// Unlike a gauge, where the value is only ever measured at a point in time, value histogram
    /// measure values over time, and their distribution.  This is nearly identical to timing
    /// histograms, since the end result is just a single number, but we don't spice it up with
    /// special unit labels or anything.
    ValueHistogram(ScopedKey, u64),
}

/// An integer scoped metric key.
#[derive(Clone, Hash, PartialEq, Eq, Debug)]
pub(crate) struct ScopedKey(pub u64, pub MetricKey);

impl ScopedKey {
    pub(crate) fn id(&self) -> u64 { self.0 }
    pub(crate) fn into_string_scoped(self, scope: String) -> StringScopedKey { StringScopedKey(scope, self.1) }
}

/// A string scoped metric key.
#[derive(Clone, Hash, PartialEq, Eq, Debug)]
pub(crate) struct StringScopedKey(String, MetricKey);

impl Display for StringScopedKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.0.is_empty() {
            write!(f, "{}", self.1)
        } else {
            write!(f, "{}.{}", self.0, self.1.as_ref())
        }
    }
}
