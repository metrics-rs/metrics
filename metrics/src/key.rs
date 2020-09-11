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

    /// Name of this key.
    pub fn name(&self) -> &ScopedString {
        &self.name
    }

    /// Labels of this key, if they exist.
    pub fn labels(&self) -> Iter<Label> {
        self.labels.iter()
    }

    /// Map the name of this key to a new name, based on `f`.
    ///
    /// The value returned by `f` becomes the new name of the key.
    pub fn map_name<F>(mut self, f: F) -> Self
    where
        F: Fn(ScopedString) -> String,
    {
        let new_name = f(self.name);
        self.name = new_name.into();
        self
    }

    /// Consumes this `Key`, returning the name and any labels.
    pub fn into_parts(self) -> (ScopedString, Vec<Label>) {
        (self.name, self.labels)
    }

    /// Returns a clone of this key with some additional labels.
    pub fn with_extra_labels(&self, extra_labels: Vec<Label>) -> Self {
        if extra_labels.is_empty() {
            return self.clone();
        }

        let name = self.name.clone();
        let mut labels = self.labels.clone();
        labels.extend(extra_labels);

        Self { name, labels }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.labels.is_empty() {
            write!(f, "Key({})", self.name)
        } else {
            write!(f, "Key({}, [", self.name)?;
            let mut first = true;
            for label in &self.labels {
                if first {
                    write!(f, "{} = {}", label.0, label.1)?;
                    first = false;
                } else {
                    write!(f, ", {} = {}", label.0, label.1)?;
                }
            }
            write!(f, "])")
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

impl<N, L> From<(N, L)> for Key
where
    N: Into<ScopedString>,
    L: IntoLabels,
{
    fn from(parts: (N, L)) -> Key {
        Key::from_name_and_labels(parts.0, parts.1)
    }
}

#[warn(missing_docs)]
#[derive(Debug, Hash, Clone)]
pub enum KeyRef {
    Borrowed(&'static Key),
    Owned(Key),
}

impl PartialEq for KeyRef {
    /// We deliberately hide the differences between the containment types.
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl Eq for KeyRef {}

impl KeyRef {
    #[warn(missing_docs)]
    pub fn into_owned(self) -> Key {
        match self {
            Self::Borrowed(val) => val.clone(),
            Self::Owned(val) => val,
        }
    }
}

impl std::ops::Deref for KeyRef {
    type Target = Key;

    #[must_use]
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Borrowed(val) => val,
            Self::Owned(val) => val,
        }
    }
}

impl AsRef<Key> for KeyRef {
    #[must_use]
    fn as_ref(&self) -> &Key {
        match self {
            Self::Borrowed(val) => val,
            Self::Owned(val) => val,
        }
    }
}

impl fmt::Display for KeyRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Borrowed(val) => val.fmt(f),
            Self::Owned(val) => val.fmt(f),
        }
    }
}

// Here we don't provide generic `From` impls
// (i.e. `impl <T: Into<Key>> From<T> for KeyRef`) because the decision whether
// to construct the owned or borrowed ref is important for performance, and
// we want users of this type to explicitly make this decision rather than rely
// on the the magic of `.into()`.

impl From<Key> for KeyRef {
    fn from(key: Key) -> Self {
        Self::Owned(key)
    }
}

impl From<&'static Key> for KeyRef {
    fn from(key: &'static Key) -> Self {
        Self::Borrowed(key)
    }
}

#[warn(missing_docs)]
pub type OnceKey = once_cell::sync::OnceCell<Key>;

#[cfg(test)]
mod tests {
    use super::{Key, KeyRef, OnceKey};
    use crate::Label;

    #[test]
    fn test_key_proper_display() {
        let key1 = Key::from_name("foobar");
        let result1 = key1.to_string();
        assert_eq!(result1, "Key(foobar)");

        let key2 = Key::from_name_and_labels("foobar", vec![Label::new("system", "http")]);
        let result2 = key2.to_string();
        assert_eq!(result2, "Key(foobar, [system = http])");

        let key3 = Key::from_name_and_labels(
            "foobar",
            vec![Label::new("system", "http"), Label::new("user", "joe")],
        );
        let result3 = key3.to_string();
        assert_eq!(result3, "Key(foobar, [system = http, user = joe])");

        let key4 = Key::from_name_and_labels(
            "foobar",
            vec![
                Label::new("black", "black"),
                Label::new("lives", "lives"),
                Label::new("matter", "matter"),
            ],
        );
        let result4 = key4.to_string();
        assert_eq!(
            result4,
            "Key(foobar, [black = black, lives = lives, matter = matter])"
        );
    }

    #[test]
    fn key_ref_equality() {
        let owned_a = Key::from_name("a");
        let owned_b = Key::from_name("b");

        static STATIC_A: OnceKey = OnceKey::new();
        static STATIC_B: OnceKey = OnceKey::new();

        let borrowed_a = STATIC_A.get_or_init(|| owned_a.clone());
        let borrowed_b = STATIC_B.get_or_init(|| owned_b.clone());

        assert_eq!(
            KeyRef::Owned(owned_a.clone()),
            KeyRef::Owned(owned_a.clone())
        );
        assert_eq!(
            KeyRef::Owned(owned_b.clone()),
            KeyRef::Owned(owned_b.clone())
        );

        assert_eq!(KeyRef::Borrowed(borrowed_a), KeyRef::Borrowed(borrowed_a));
        assert_eq!(KeyRef::Borrowed(borrowed_b), KeyRef::Borrowed(borrowed_b));

        assert_eq!(KeyRef::Owned(owned_a.clone()), KeyRef::Borrowed(borrowed_a));
        assert_eq!(KeyRef::Owned(owned_b.clone()), KeyRef::Borrowed(borrowed_b));

        assert_eq!(KeyRef::Borrowed(borrowed_a), KeyRef::Owned(owned_a.clone()));
        assert_eq!(KeyRef::Borrowed(borrowed_b), KeyRef::Owned(owned_b.clone()));

        assert_ne!(
            KeyRef::Owned(owned_a.clone()),
            KeyRef::Owned(owned_b.clone()),
        );
        assert_ne!(KeyRef::Borrowed(borrowed_a), KeyRef::Borrowed(borrowed_b));
        assert_ne!(KeyRef::Owned(owned_a.clone()), KeyRef::Borrowed(borrowed_b));
        assert_ne!(KeyRef::Owned(owned_b.clone()), KeyRef::Borrowed(borrowed_a));
    }
}
