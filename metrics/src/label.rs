use crate::ScopedString;

/// A key/value pair used to further describe a metric.
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct Label(pub(crate) ScopedString, pub(crate) ScopedString);

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