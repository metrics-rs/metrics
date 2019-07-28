use crate::data::AtomicWindowedHistogram;
use arc_swap::ArcSwapOption;
use metrics_core::Key;
use metrics_util::StreamingIntegers;
use quanta::Clock;
use std::{
    fmt,
    ops::Deref,
    sync::{
        atomic::{AtomicI64, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

/// A scope, or context, for a metric.
#[doc(hidden)]
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub enum Scope {
    /// Root scope.
    Root,

    /// A nested scope, with arbitrarily deep nesting.
    Nested(Vec<String>),
}

impl Scope {
    /// Adds a new part to this scope.
    pub fn add_part<S>(self, part: S) -> Self
    where
        S: Into<String>,
    {
        match self {
            Scope::Root => Scope::Nested(vec![part.into()]),
            Scope::Nested(mut parts) => {
                parts.push(part.into());
                Scope::Nested(parts)
            }
        }
    }

    pub(crate) fn into_string<S>(self, name: S) -> String
    where
        S: Into<String>,
    {
        match self {
            Scope::Root => name.into(),
            Scope::Nested(mut parts) => {
                parts.push(name.into());
                parts.join(".")
            }
        }
    }
}

pub(crate) type ScopeHandle = u64;

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub(crate) enum Kind {
    Counter,
    Gauge,
    Histogram,
    Proxy,
}

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub(crate) struct Identifier(Key, ScopeHandle, Kind);

impl Identifier {
    pub fn new<K>(key: K, handle: ScopeHandle, kind: Kind) -> Self
    where
        K: Into<Key>,
    {
        Identifier(key.into(), handle, kind)
    }

    pub fn kind(&self) -> Kind {
        self.2.clone()
    }

    pub fn into_parts(self) -> (Key, ScopeHandle, Kind) {
        (self.0, self.1, self.2)
    }
}

#[derive(Debug)]
enum ValueState {
    Counter(AtomicU64),
    Gauge(AtomicI64),
    Histogram(AtomicWindowedHistogram),
    Proxy(ArcSwapOption<Box<ProxyFn>>),
}

#[derive(Debug)]
pub(crate) enum ValueSnapshot {
    Single(Measurement),
    Multiple(Vec<(Key, Measurement)>),
}

/// A point-in-time metric measurement.
#[derive(Debug)]
pub enum Measurement {
    /// Counters represent a single value that can only ever be incremented over time, or reset to
    /// zero.
    Counter(u64),
    /// Gauges represent a single value that can go up _or_ down over time.
    Gauge(i64),
    /// Histograms measure the distribution of values for a given set of measurements.
    ///
    /// Histograms are slightly special in our case because we want to maintain full fidelity of
    /// the underlying dataset.  We do this by storing all of the individual data points, but we
    /// use [`StreamingIntegers`] to store them in a compressed in-memory form.  This allows
    /// callers to pass around the compressed dataset and decompress/access the actual integers on
    /// demand.
    Histogram(StreamingIntegers),
}

#[derive(Clone, Debug)]
/// Handle to the underlying measurement for a metric.
pub(crate) struct ValueHandle {
    state: Arc<ValueState>,
}

impl ValueHandle {
    fn new(state: ValueState) -> Self {
        ValueHandle {
            state: Arc::new(state),
        }
    }

    pub fn counter() -> Self {
        Self::new(ValueState::Counter(AtomicU64::new(0)))
    }

    pub fn gauge() -> Self {
        Self::new(ValueState::Gauge(AtomicI64::new(0)))
    }

    pub fn histogram(window: Duration, granularity: Duration, clock: Clock) -> Self {
        Self::new(ValueState::Histogram(AtomicWindowedHistogram::new(
            window,
            granularity,
            clock,
        )))
    }

    pub fn proxy() -> Self {
        Self::new(ValueState::Proxy(ArcSwapOption::new(None)))
    }

    pub fn update_counter(&self, value: u64) {
        match self.state.deref() {
            ValueState::Counter(inner) => {
                inner.fetch_add(value, Ordering::Release);
            }
            _ => unreachable!("tried to access as counter, not a counter"),
        }
    }

    pub fn update_gauge(&self, value: i64) {
        match self.state.deref() {
            ValueState::Gauge(inner) => inner.store(value, Ordering::Release),
            _ => unreachable!("tried to access as gauge, not a gauge"),
        }
    }

    pub fn update_histogram(&self, value: u64) {
        match self.state.deref() {
            ValueState::Histogram(inner) => inner.record(value),
            _ => unreachable!("tried to access as histogram, not a histogram"),
        }
    }

    pub fn update_proxy<F>(&self, value: F)
    where
        F: Fn() -> Vec<(Key, Measurement)> + Send + Sync + 'static,
    {
        match self.state.deref() {
            ValueState::Proxy(inner) => {
                inner.store(Some(Arc::new(Box::new(value))));
            }
            _ => unreachable!("tried to access as proxy, not a proxy"),
        }
    }

    pub fn snapshot(&self) -> ValueSnapshot {
        match self.state.deref() {
            ValueState::Counter(inner) => {
                let value = inner.load(Ordering::Acquire);
                ValueSnapshot::Single(Measurement::Counter(value))
            }
            ValueState::Gauge(inner) => {
                let value = inner.load(Ordering::Acquire);
                ValueSnapshot::Single(Measurement::Gauge(value))
            }
            ValueState::Histogram(inner) => {
                let stream = inner.snapshot();
                ValueSnapshot::Single(Measurement::Histogram(stream))
            }
            ValueState::Proxy(maybe) => {
                let measurements = match maybe.load() {
                    None => Vec::new(),
                    Some(f) => f(),
                };

                ValueSnapshot::Multiple(measurements)
            }
        }
    }
}

/// Trait for types that represent time and can be subtracted from each other to generate a delta.
pub trait Delta {
    /// Get the delta between this value and another value.
    ///
    /// For `Instant`, we explicitly return the nanosecond difference.  For `u64`, we return the
    /// integer difference, but the timescale itself can be whatever the user desires.
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

pub trait ProxyFnInner: Fn() -> Vec<(Key, Measurement)> {}
impl<F> ProxyFnInner for F where F: Fn() -> Vec<(Key, Measurement)> {}

pub type ProxyFn = dyn ProxyFnInner<Output = Vec<(Key, Measurement)>> + Send + Sync + 'static;

impl fmt::Debug for ProxyFn {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ProxyFn")
    }
}

#[cfg(test)]
mod tests {
    use super::{Measurement, Scope, ValueHandle, ValueSnapshot};
    use metrics_core::Key;
    use quanta::Clock;
    use std::borrow::Cow;
    use std::time::Duration;

    #[test]
    fn test_metric_scope() {
        let root_scope = Scope::Root;
        assert_eq!(root_scope.into_string(""), "".to_string());

        let root_scope = Scope::Root;
        assert_eq!(root_scope.into_string("jambalaya"), "jambalaya".to_string());

        let nested_scope = Scope::Nested(vec![]);
        assert_eq!(nested_scope.into_string(""), "".to_string());

        let nested_scope = Scope::Nested(vec![]);
        assert_eq!(nested_scope.into_string("toilet"), "toilet".to_string());

        let nested_scope = Scope::Nested(vec!["chamber".to_string(), "of".to_string()]);
        assert_eq!(
            nested_scope.into_string("secrets"),
            "chamber.of.secrets".to_string()
        );

        let nested_scope = Scope::Nested(vec![
            "chamber".to_string(),
            "of".to_string(),
            "secrets".to_string(),
        ]);
        assert_eq!(
            nested_scope.into_string("toilet"),
            "chamber.of.secrets.toilet".to_string()
        );

        let mut nested_scope = Scope::Root;
        nested_scope = nested_scope
            .add_part("chamber")
            .add_part("of".to_string())
            .add_part(Cow::Borrowed("secrets"));
        assert_eq!(
            nested_scope.into_string(""),
            "chamber.of.secrets.".to_string()
        );

        let mut nested_scope = Scope::Nested(vec![
            "chamber".to_string(),
            "of".to_string(),
            "secrets".to_string(),
        ]);
        nested_scope = nested_scope.add_part("part");
        assert_eq!(
            nested_scope.into_string("two"),
            "chamber.of.secrets.part.two".to_string()
        );
    }

    #[test]
    fn test_metric_values() {
        let counter = ValueHandle::counter();
        counter.update_counter(42);
        match counter.snapshot() {
            ValueSnapshot::Single(Measurement::Counter(value)) => assert_eq!(value, 42),
            _ => panic!("incorrect value snapshot type for counter"),
        }

        let gauge = ValueHandle::gauge();
        gauge.update_gauge(23);
        match gauge.snapshot() {
            ValueSnapshot::Single(Measurement::Gauge(value)) => assert_eq!(value, 23),
            _ => panic!("incorrect value snapshot type for gauge"),
        }

        let (mock, _) = Clock::mock();
        let histogram =
            ValueHandle::histogram(Duration::from_secs(10), Duration::from_secs(1), mock);
        histogram.update_histogram(8675309);
        histogram.update_histogram(5551212);
        match histogram.snapshot() {
            ValueSnapshot::Single(Measurement::Histogram(stream)) => {
                assert_eq!(stream.len(), 2);

                let values = stream.decompress();
                assert_eq!(&values[..], [8675309, 5551212]);
            }
            _ => panic!("incorrect value snapshot type for histogram"),
        }

        let proxy = ValueHandle::proxy();
        proxy.update_proxy(|| vec![(Key::from_name("foo"), Measurement::Counter(23))]);
        match proxy.snapshot() {
            ValueSnapshot::Multiple(mut measurements) => {
                assert_eq!(measurements.len(), 1);

                let measurement = measurements.pop().expect("should have measurement");
                assert_eq!(measurement.0.name().as_ref(), "foo");
                match measurement.1 {
                    Measurement::Counter(i) => assert_eq!(i, 23),
                    _ => panic!("wrong measurement type"),
                }
            }
            _ => panic!("incorrect value snapshot type for proxy"),
        }

        // This second one just makes sure that replacing the proxy function functions as intended.
        proxy.update_proxy(|| vec![(Key::from_name("bar"), Measurement::Counter(24))]);
        match proxy.snapshot() {
            ValueSnapshot::Multiple(mut measurements) => {
                assert_eq!(measurements.len(), 1);

                let measurement = measurements.pop().expect("should have measurement");
                assert_eq!(measurement.0.name().as_ref(), "bar");
                match measurement.1 {
                    Measurement::Counter(i) => assert_eq!(i, 24),
                    _ => panic!("wrong measurement type"),
                }
            }
            _ => panic!("incorrect value snapshot type for proxy"),
        }
    }
}
