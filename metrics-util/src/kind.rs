use std::ops::BitOr;

/// Metric kind.
///
/// Defines the kind, or type, of a metric.  Follows the supported metric types of `metrics`:
/// - counters
/// - gauges
/// - histograms
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum MetricKind {
    /// Counter type.
    Counter,
    /// Gauge type.
    Gauge,
    /// Histogram type.
    Histogram,
}

/// Metric kind mask.
///
/// Useful for matching against a kind, or kinds, of metrics.
///
/// In order to use for defining multiple metric kinds, can be used in a bitmask fashion, as this
/// type implements bitwise OR support, and checking for inclusion of a specific kind within another
/// kind value can be checked via [`matches`](MetricKindMask::matches):
///
/// ```rust
/// # use metrics_util::{MetricKind, MetricKindMask};
/// # fn main() {
/// // Let's only match counters and histograms:
/// let mask = MetricKindMask::COUNTER | MetricKindMask::HISTOGRAM;
///
/// // And check to see if the kinds we have matches our mask:
/// assert!(!mask.matches(MetricKind::Gauge));
/// assert!(mask.matches(MetricKind::Counter));
///
/// // There's even two handy versions to avoid extra typing:
/// let none_mask = MetricKindMask::NONE;
/// let all_mask = MetricKindMask::ALL;
///
/// assert!(!none_mask.matches(MetricKind::Counter));
/// assert!(!none_mask.matches(MetricKind::Gauge));
/// assert!(!none_mask.matches(MetricKind::Histogram));
/// assert!(all_mask.matches(MetricKind::Counter));
/// assert!(all_mask.matches(MetricKind::Gauge));
/// assert!(all_mask.matches(MetricKind::Histogram));
/// # }
/// ```
#[derive(Debug, Eq, PartialEq, Hash, Clone, Copy, Ord, PartialOrd)]
pub struct MetricKindMask(u8);

impl MetricKindMask {
    /// No metric kinds.
    pub const NONE: MetricKindMask = MetricKindMask(0);

    /// The counter kind.
    pub const COUNTER: MetricKindMask = MetricKindMask(1);

    /// The gauge kind.
    pub const GAUGE: MetricKindMask = MetricKindMask(2);

    /// The histogram kind.
    pub const HISTOGRAM: MetricKindMask = MetricKindMask(4);

    /// All metric kinds.
    pub const ALL: MetricKindMask = MetricKindMask(7);

    #[inline]
    fn value(&self) -> u8 {
        self.0
    }

    /// Whether or not this metric kind contains the specified kind.
    pub fn matches(&self, kind: MetricKind) -> bool {
        match kind {
            MetricKind::Counter => self.0 & MetricKindMask::COUNTER.value() != 0,
            MetricKind::Gauge => self.0 & MetricKindMask::GAUGE.value() != 0,
            MetricKind::Histogram => self.0 & MetricKindMask::HISTOGRAM.value() != 0,
        }
    }
}

impl BitOr for MetricKindMask {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

#[cfg(test)]
mod tests {
    use super::MetricKindMask;
    use crate::MetricKind;

    #[test]
    fn test_matching() {
        let cmask = MetricKindMask::COUNTER;
        let gmask = MetricKindMask::GAUGE;
        let hmask = MetricKindMask::HISTOGRAM;
        let nmask = MetricKindMask::NONE;
        let amask = MetricKindMask::ALL;

        assert!(cmask.matches(MetricKind::Counter));
        assert!(!cmask.matches(MetricKind::Gauge));
        assert!(!cmask.matches(MetricKind::Histogram));

        assert!(!gmask.matches(MetricKind::Counter));
        assert!(gmask.matches(MetricKind::Gauge));
        assert!(!gmask.matches(MetricKind::Histogram));

        assert!(!hmask.matches(MetricKind::Counter));
        assert!(!hmask.matches(MetricKind::Gauge));
        assert!(hmask.matches(MetricKind::Histogram));

        assert!(amask.matches(MetricKind::Counter));
        assert!(amask.matches(MetricKind::Gauge));
        assert!(amask.matches(MetricKind::Histogram));

        assert!(!nmask.matches(MetricKind::Counter));
        assert!(!nmask.matches(MetricKind::Gauge));
        assert!(!nmask.matches(MetricKind::Histogram));
    }
}
