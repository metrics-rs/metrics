use crate::{IntoLabels, Label, ScopedString};
use std::{fmt, slice::Iter};

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
            write!(f, "Key({})", self.name)
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
