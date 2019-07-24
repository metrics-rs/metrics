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
use std::{borrow::Cow, fmt, slice::Iter, time::Duration};

/// An allocation-optimized string.
///
/// We specify `ScopedString` to attempt to get the best of both worlds: flexibility to provide a
/// static or dynamic (owned) string, while retaining the performance benefits of being able to
/// take ownership of owned strings and borrows of completely static strings.
pub type ScopedString = Cow<'static, str>;

/// A key/value pair used to further describe a metric.
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct Label(ScopedString, ScopedString);

impl Label {
    /// Creates a `Label` from a key and value.
    pub fn new<K, V>(key: K, value: V) -> Self
    where
        K: Into<ScopedString>,
        V: Into<ScopedString>,
    {
        Label(key.into(), value.into())
    }

    /// The key of this label.
    pub fn key(&self) -> &str {
        self.0.as_ref()
    }

    /// The value of this label.
    pub fn value(&self) -> &str {
        self.1.as_ref()
    }

    /// Consumes this `Label`, returning the key and value.
    pub fn into_parts(self) -> (ScopedString, ScopedString) {
        (self.0, self.1)
    }
}

/// A metric key.
///
/// A key always includes a name, but can optional include multiple labels used to further describe
/// the metric.
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct Key {
    name: ScopedString,
    labels: Vec<Label>,
}

impl Key {
    /// Creates a `Key` from a name.
    pub fn from_name<N>(name: N) -> Self
    where
        N: Into<ScopedString>,
    {
        Key {
            name: name.into(),
            labels: Vec::new(),
        }
    }

    /// Creates a `Key` from a name and vector of `Label`s.
    pub fn from_name_and_labels<N, L>(name: N, labels: L) -> Self
    where
        N: Into<ScopedString>,
        L: IntoLabels,
    {
        Key {
            name: name.into(),
            labels: labels.into_labels(),
        }
    }

    /// Adds a new set of labels to this key.
    ///
    /// New labels will be appended to any existing labels.
    pub fn add_labels<L>(&mut self, new_labels: L)
    where
        L: IntoLabels,
    {
        self.labels.extend(new_labels.into_labels());
    }

    /// Name of this key.
    pub fn name(&self) -> ScopedString {
        self.name.clone()
    }

    /// Labels of this key, if they exist.
    pub fn labels(&self) -> Iter<Label> {
        self.labels.iter()
    }

    /// Maps the name of this `Key` to a new name.
    pub fn map_name<F, S>(self, f: F) -> Self
    where
        F: FnOnce(ScopedString) -> S,
        S: Into<ScopedString>,
    {
        Key {
            name: f(self.name).into(),
            labels: self.labels,
        }
    }

    /// Consumes this `Key`, returning the name and any labels.
    pub fn into_parts(self) -> (ScopedString, Vec<Label>) {
        (self.name, self.labels)
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.labels.is_empty() {
            write!(f, "Key({}", self.name)
        } else {
            let kv_pairs = self
                .labels
                .iter()
                .map(|label| format!("{} = {}", label.0, label.1))
                .collect::<Vec<_>>();
            write!(f, "Key({}, [{}])", self.name, kv_pairs.join(", "))
        }
    }
}

impl From<String> for Key {
    fn from(name: String) -> Key {
        Key::from_name(name)
    }
}

impl From<&'static str> for Key {
    fn from(name: &'static str) -> Key {
        Key::from_name(name)
    }
}

impl From<ScopedString> for Key {
    fn from(name: ScopedString) -> Key {
        Key::from_name(name)
    }
}

impl<K, L> From<(K, L)> for Key
where
    K: Into<ScopedString>,
    L: IntoLabels,
{
    fn from(parts: (K, L)) -> Key {
        Key::from_name_and_labels(parts.0, parts.1)
    }
}

impl<K, V> From<(K, V)> for Label
where
    K: Into<ScopedString>,
    V: Into<ScopedString>,
{
    fn from(pair: (K, V)) -> Label {
        Label::new(pair.0, pair.1)
    }
}

impl<K, V> From<&(K, V)> for Label
where
    K: Into<ScopedString> + Clone,
    V: Into<ScopedString> + Clone,
{
    fn from(pair: &(K, V)) -> Label {
        Label::new(pair.0.clone(), pair.1.clone())
    }
}

/// A value that can be converted to `Label`s.
pub trait IntoLabels {
    /// Consumes this value, turning it into a vector of `Label`s.
    fn into_labels(self) -> Vec<Label>;
}

impl IntoLabels for Vec<Label> {
    fn into_labels(self) -> Vec<Label> {
        self
    }
}

impl<T, L> IntoLabels for &T
where
    Self: IntoIterator<Item = L>,
    L: Into<Label>,
{
    fn into_labels(self) -> Vec<Label> {
        self.into_iter().map(|l| l.into()).collect()
    }
}

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

/// A value that observes metrics.
pub trait Observer {
    /// The method called when a counter is observed.
    ///
    /// From the perspective of an observer, a counter and gauge are essentially identical, insofar
    /// as they are both a single value tied to a key.  From the perspective of a collector,
    /// counters and gauges usually have slightly different modes of operation.
    ///
    /// For the sake of flexibility on the exporter side, both are provided.
    fn observe_counter(&mut self, key: Key, value: u64);

    /// The method called when a gauge is observed.
    ///
    /// From the perspective of a observer, a counter and gauge are essentially identical, insofar
    /// as they are both a single value tied to a key.  From the perspective of a collector,
    /// counters and gauges usually have slightly different modes of operation.
    ///
    /// For the sake of flexibility on the exporter side, both are provided.
    fn observe_gauge(&mut self, key: Key, value: i64);

    /// The method called when an histogram is observed.
    ///
    /// Observers are expected to tally their own histogram views, so this will be called with all
    /// of the underlying observed values, and callers will need to process them accordingly.
    ///
    /// There is no guarantee that this method will not be called multiple times for the same key.
    fn observe_histogram(&mut self, key: Key, values: &[u64]);
}

/// A value that can build an observer.
///
/// Observers are containers used for rendering a snapshot in a particular format.
/// As many systems are multi-threaded, we can't easily share a single recorder amongst
/// multiple threads, and so we create a recorder per observation, tying them together.
///
/// A builder allows us to generate an observer on demand, giving each specific recorder an
/// interface by which they can do any necessary configuration, initialization, etc of the
/// observer before handing it over to the exporter.
pub trait Builder {
    /// The observer created by this builder.
    type Output: Observer;

    /// Creates a new recorder.
    fn build(&self) -> Self::Output;
}

/// A value that can produce a `T` by draining its content.
///
/// After being drained, the value should be ready to be reused.
pub trait Drain<T> {
    /// Drain the `Observer`, producing a `T`.
    fn drain(&mut self) -> T;
}

/// A value whose metrics can be observed by an `Observer`.
pub trait Observe {
    /// Observe point-in-time view of the collected metrics.
    fn observe<O: Observer>(&self, observer: &mut O);
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
        labels!(@ { $crate::Label::new($k, $v) } $($rest)*)
    };

    (@ { $($out:expr),+ } $k:expr => $v:expr, $($rest:tt)*) => {
        labels!(@ { $($out),+, $crate::Label::new($k, $v) } $($rest)*)
    };

    ($($args:tt)*) => {
        labels!(@ { } $($args)*, )
    };
}
