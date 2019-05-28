use crate::data::AtomicWindowedHistogram;
use metrics_util::StreamingIntegers;
use quanta::Clock;
use std::borrow::Cow;
use std::ops::Deref;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Optimized metric name.
///
/// This can either be a [`&'static str`](str) or [`String`].
pub type MetricName = Cow<'static, str>;

/// A scope, or context, for a metric.
///
/// Not interacted with directly by end users, and only exposed due to a lack of trait method
/// visbility controls.
///
/// See also: [Sink::scoped](crate::Sink::scoped).
#[derive(PartialEq, Eq, Hash, Clone)]
pub enum MetricScope {
    Root,
    Nested(Vec<String>),
}

impl MetricScope {
    pub(crate) fn into_scoped(self, name: MetricName) -> String {
        match self {
            MetricScope::Root => name.to_string(),
            MetricScope::Nested(mut parts) => {
                if !name.is_empty() {
                    parts.push(name.to_string());
                }
                parts.join(".")
            }
        }
    }
}

pub(crate) type MetricScopeHandle = u64;

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub(crate) enum MetricKind {
    Counter,
    Gauge,
    Histogram,
}

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub(crate) enum MetricIdentifier {
    Unlabeled(MetricName, MetricScopeHandle, MetricKind),
}

#[derive(Debug)]
enum ValueState {
    Counter(AtomicU64),
    Gauge(AtomicI64),
    Histogram(AtomicWindowedHistogram),
}

#[derive(Debug)]
pub(crate) enum ValueSnapshot {
    Counter(u64),
    Gauge(i64),
    Histogram(StreamingIntegers),
}

#[derive(Clone, Debug)]
/// Handle to the underlying measurement for a metric.
pub(crate) struct MetricValue {
    state: Arc<ValueState>,
}

impl MetricValue {
    fn new(state: ValueState) -> Self {
        MetricValue {
            state: Arc::new(state),
        }
    }

    pub(crate) fn new_counter() -> Self {
        Self::new(ValueState::Counter(AtomicU64::new(0)))
    }

    pub(crate) fn new_gauge() -> Self {
        Self::new(ValueState::Gauge(AtomicI64::new(0)))
    }

    pub(crate) fn new_histogram(window: Duration, granularity: Duration, clock: Clock) -> Self {
        Self::new(ValueState::Histogram(AtomicWindowedHistogram::new(
            window,
            granularity,
            clock,
        )))
    }

    pub(crate) fn update_counter(&self, value: u64) {
        match self.state.deref() {
            ValueState::Counter(inner) => {
                inner.fetch_add(value, Ordering::Release);
            }
            _ => unreachable!("tried to access as counter, not a counter"),
        }
    }

    pub(crate) fn update_gauge(&self, value: i64) {
        match self.state.deref() {
            ValueState::Gauge(inner) => inner.store(value, Ordering::Release),
            _ => unreachable!("tried to access as gauge, not a gauge"),
        }
    }

    pub(crate) fn update_histogram(&self, value: u64) {
        match self.state.deref() {
            ValueState::Histogram(inner) => inner.record(value),
            _ => unreachable!("tried to access as histogram, not a histogram"),
        }
    }

    pub(crate) fn snapshot(&self) -> ValueSnapshot {
        match self.state.deref() {
            ValueState::Counter(inner) => {
                let value = inner.load(Ordering::Acquire);
                ValueSnapshot::Counter(value)
            }
            ValueState::Gauge(inner) => {
                let value = inner.load(Ordering::Acquire);
                ValueSnapshot::Gauge(value)
            }
            ValueState::Histogram(inner) => {
                let stream = inner.snapshot();
                ValueSnapshot::Histogram(stream)
            }
        }
    }
}

/// Trait for types that represent time and can be subtracted from each other to generate a delta.
pub trait Delta {
    fn delta(&self, other: Self) -> u64;
}

impl Delta for u64 {
    fn delta(&self, other: u64) -> u64 {
        self.wrapping_sub(other)
    }
}

impl Delta for Instant {
    fn delta(&self, other: Instant) -> u64 {
        let dur = *self - other;
        dur.as_nanos() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::{MetricScope, MetricValue, ValueSnapshot};
    use quanta::Clock;
    use std::time::Duration;

    #[test]
    fn test_metric_scope() {
        let root_scope = MetricScope::Root;
        assert_eq!(root_scope.into_scoped("".into()), "".to_string());

        let root_scope = MetricScope::Root;
        assert_eq!(
            root_scope.into_scoped("jambalaya".into()),
            "jambalaya".to_string()
        );

        let nested_scope = MetricScope::Nested(vec![]);
        assert_eq!(nested_scope.into_scoped("".into()), "".to_string());

        let nested_scope = MetricScope::Nested(vec![]);
        assert_eq!(
            nested_scope.into_scoped("toilet".into()),
            "toilet".to_string()
        );

        let nested_scope = MetricScope::Nested(vec![
            "chamber".to_string(),
            "of".to_string(),
            "secrets".to_string(),
        ]);
        assert_eq!(
            nested_scope.into_scoped("".into()),
            "chamber.of.secrets".to_string()
        );

        let nested_scope = MetricScope::Nested(vec![
            "chamber".to_string(),
            "of".to_string(),
            "secrets".to_string(),
        ]);
        assert_eq!(
            nested_scope.into_scoped("toilet".into()),
            "chamber.of.secrets.toilet".to_string()
        );
    }

    #[test]
    fn test_metric_values() {
        let counter = MetricValue::new_counter();
        counter.update_counter(42);
        match counter.snapshot() {
            ValueSnapshot::Counter(value) => assert_eq!(value, 42),
            _ => panic!("incorrect value snapshot type for counter"),
        }

        let gauge = MetricValue::new_gauge();
        gauge.update_gauge(23);
        match gauge.snapshot() {
            ValueSnapshot::Gauge(value) => assert_eq!(value, 23),
            _ => panic!("incorrect value snapshot type for gauge"),
        }

        let (mock, _) = Clock::mock();
        let histogram =
            MetricValue::new_histogram(Duration::from_secs(10), Duration::from_secs(1), mock);
        histogram.update_histogram(8675309);
        histogram.update_histogram(5551212);
        match histogram.snapshot() {
            ValueSnapshot::Histogram(stream) => {
                assert_eq!(stream.len(), 2);

                let values = stream.decompress();
                assert_eq!(&values[..], [8675309, 5551212]);
            }
            _ => panic!("incorrect value snapshot type for histogram"),
        }
    }
}
