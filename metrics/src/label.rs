use std::fmt;
use crate::ScopedString;

/// A key/value pair used to further describe a metric.
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Debug)]
pub enum Label {
    /// Key and value are both static and do not change ever.
    Static(ScopedString, ScopedString),

    /// Key is always static, but value can be changed after construction.
    Dynamic(ScopedString, Option<ScopedString>),
}

impl Label {
    /// Creates a static `Label` from a known key and value.
    pub fn from_static<K, V>(key: K, value: V) -> Self
    where
        K: Into<ScopedString>,
        V: Into<ScopedString>,
    {
        Label::Static(key.into(), value.into())
    }

    /// Creates a dynamic `Label` from a known key but no value.
    pub fn from_dynamic<K>(key: K) -> Self
    where
        K: Into<ScopedString>,
    {
        Label::Dynamic(key.into(), None)
    }

    /// Creates a dynamic `Label` from a known key and value.
    ///
    /// Primarily used for testing.
    pub fn from_dynamic_with_value<K, V>(key: K, value: V) -> Self
    where
        K: Into<ScopedString>,
        V: Into<ScopedString>,
    {
        Label::Dynamic(key.into(), Some(value.into()))
    }

    /// Whether or not this label lacks a value.
    pub fn requires_value(&self) -> bool {
        match self {
            Label::Dynamic(_, value) => value.is_none(),
            _ => false,
        }
    }
    
    /// Sets the value of this label.
    /// 
    /// If the label is static, this method has no effect.
    pub fn set_value<V>(&mut self, value: V)
    where
        V: Into<ScopedString>
    {
        match self {
            Label::Dynamic(_, old) => {
                let _ = old.replace(value.into());
            },
            _ => {}
        }
    }

    /// The key of this label.
    pub fn key(&self) -> &str {
        match self {
            Label::Static(key, _) => key.as_ref(),
            Label::Dynamic(key, _) => key.as_ref(),
        }
    }

    /// The value of this label, if set.
    pub fn value(&self) -> Option<&str> {
        match self {
            Label::Static(_, value) => Some(value.as_ref()),
            Label::Dynamic(_, value) => value.as_ref().map(|s| s.as_ref()),
        }
    }

    /// Consumes this `Label`, returning the key and value.
    pub fn into_parts(self) -> (ScopedString, Option<ScopedString>) {
        match self {
            Label::Static(key, value) => (key, Some(value)),
            Label::Dynamic(key, value) => (key, value),
        }
    }
}

impl fmt::Display for Label {
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Label::Static(key, value) => write!(w, "{} => {}", key, value),
            Label::Dynamic(key, value) => match value {
                Some(value2) => write!(w, "{} => {}", key, value2),
                None => write!(w, "unresolved(\"{}\")", key),
            }
        }
    }
}

impl<K, V> From<&(K, V)> for Label
where
    K: Into<ScopedString> + Clone,
    V: Into<ScopedString> + Clone,
{
    fn from(pair: &(K, V)) -> Label {
        Label::from_static(pair.0.clone(), pair.1.clone())
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
