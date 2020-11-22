use std::ops::BitOr;

/// Metric kind.
///
/// Useful for defining the kind of a metric in metadata, or creating a sum of metric kinds to
/// describe allowed metric kinds for a specific operation, etc.
///
/// In order to use for defining multiple metric kinds, can be used in a bitmask fashion, as this
/// type implements bitwise OR support, and checking for inclusion of a specific kind within another
/// kind value can be checked via [`contains`]:
///
/// ```rust
/// # use metrics_util::MetricKind;
/// # fn main() {
/// // Let's only match counters and histograms:
/// let mask = MetricKind::COUNTER | MetricKind::HISTOGRAM;
///
/// // And check to see if the kinds we have matches our mask:
/// let some_kind = MetricKind::GAUGE;
/// let another_kind = MetricKind::COUNTER;
///
/// assert!(!mask.contains(&some_kind));
/// assert!(mask.contains(&another_kind));
///
/// // There's even two handy versions to avoid extra typing:
/// let none_mask = MetricKind::NONE;
/// let all_mask = MetricKind::ALL;
///
/// assert!(!none_mask.contains(MetricKind::COUNTER));
/// assert!(!none_mask.contains(MetricKind::GAUGE));
/// assert!(!none_mask.contains(MetricKind::HISTOGRAM));
/// assert!(all_mask.contains(MetricKind::COUNTER));
/// assert!(all_mask.contains(MetricKind::GAUGE));
/// assert!(all_mask.contains(MetricKind::HISTOGRAM));
/// # }
/// ```
#[derive(Debug, Eq, PartialEq, Hash, Clone, Copy, Ord, PartialOrd)]
pub struct MetricKind(u8);

impl MetricKind {
    /// No metric kinds.
    pub const NONE: MetricKind = MetricKind(0);

    /// The counter kind.
    pub const COUNTER: MetricKind = MetricKind(1);

    /// The gauge kind.
    pub const GAUGE: MetricKind = MetricKind(2);

    /// The histogram kind.
    pub const HISTOGRAM: MetricKind = MetricKind(4);

    /// All metric kind.
    pub const ALL: MetricKind = MetricKind(7);

    /// Whether or not this metric kind contains the specified kind.
    pub fn contains(&self, other: MetricKind) -> bool {
        self.0 & other.0 != 0
    }
}

impl BitOr for MetricKind {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}
