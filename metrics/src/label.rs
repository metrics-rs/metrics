use crate::SharedString;
use alloc::vec::Vec;

/// Metadata for a metric key in the for of a key/value pair.
///
/// Metrics are always defined by a name, but can optionally be assigned "labels", which are
/// key/value pairs that provide metadata about the key.  Labels are typically used for
/// differentiating the context of when an where a metric are emitted.
///
/// For example, in a web service, you might wish to label metrics with the user ID responsible for
/// the request currently being processed, or the request path being processed.  Another example may
/// be that if you were running a piece o code that was turned on or off by a feature toggle, you may
/// wish to include a label in metrics to indicate whether or not they were using the feature toggle.
#[derive(PartialEq, Eq, Hash, Clone, Debug, PartialOrd, Ord)]
pub struct Label(pub(crate) SharedString, pub(crate) SharedString);

impl Label {
    /// Creates a [`Label`] from a key and value.
    pub fn new<K, V>(key: K, value: V) -> Self
    where
        K: Into<SharedString>,
        V: Into<SharedString>,
    {
        Label(key.into(), value.into())
    }

    /// Creates a [`Label`] from a static key and value.
    pub const fn from_static_parts(key: &'static str, value: &'static str) -> Self {
        Label(SharedString::const_str(key), SharedString::const_str(value))
    }

    /// Key of this label.
    pub fn key(&self) -> &str {
        self.0.as_ref()
    }

    /// Value of this label.
    pub fn value(&self) -> &str {
        self.1.as_ref()
    }

    /// Consumes this [`Label`], returning the key and value.
    pub fn into_parts(self) -> (SharedString, SharedString) {
        (self.0, self.1)
    }
}

impl<K, V> From<&(K, V)> for Label
where
    K: Into<SharedString> + Clone,
    V: Into<SharedString> + Clone,
{
    fn from(pair: &(K, V)) -> Label {
        Label::new(pair.0.clone(), pair.1.clone())
    }
}

/// A value that can be converted to a vector of [`Label`]s.
pub trait IntoLabels {
    /// Consumes this value, turning it into a vector of [`Label`]s.
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
