use crate::{cow::Cow, IntoLabels, KeyHasher, Label, SharedString};
use alloc::{string::String, vec::Vec};
use core::{fmt, hash::Hash, slice::Iter};
use std::{
    cmp,
    hash::Hasher,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

const NO_LABELS: [Label; 0] = [];

/// Parts compromising a metric name.
#[derive(PartialEq, Eq, Hash, Clone, Debug, PartialOrd, Ord)]
pub struct NameParts(Cow<'static, [SharedString]>);

impl NameParts {
    /// Creates a [`NameParts`] from the given name.
    pub fn from_name<N: Into<SharedString>>(name: N) -> Self {
        NameParts(Cow::owned(vec![name.into()]))
    }

    /// Creates a [`NameParts`] from the given static name.
    pub const fn from_static_names(names: &'static [SharedString]) -> Self {
        NameParts(Cow::<'static, [SharedString]>::const_slice(names))
    }

    /// Appends a name part.
    pub fn append<S: Into<SharedString>>(self, part: S) -> Self {
        let mut parts = self.0.into_owned();
        parts.push(part.into());
        NameParts(Cow::owned(parts))
    }

    /// Prepends a name part.
    pub fn prepend<S: Into<SharedString>>(self, part: S) -> Self {
        let mut parts = self.0.into_owned();
        parts.insert(0, part.into());
        NameParts(Cow::owned(parts))
    }

    /// Gets a reference to the parts for this name.
    pub fn parts(&self) -> Iter<'_, SharedString> {
        self.0.iter()
    }

    /// Creates an owned version of these parts, joined with periods into a single string.
    pub fn to_string(&self) -> String {
        let mut first = false;
        let mut s = String::with_capacity(16);
        for p in self.0.iter() {
            if first {
                s.push('.');
                first = false;
            }
            s.push_str(p.as_ref());
        }
        s
    }
}

impl From<String> for NameParts {
    fn from(name: String) -> NameParts {
        NameParts::from_name(name)
    }
}

impl From<&'static str> for NameParts {
    fn from(name: &'static str) -> NameParts {
        NameParts::from_name(name)
    }
}

impl From<&'static [SharedString]> for NameParts {
    fn from(names: &'static [SharedString]) -> NameParts {
        NameParts::from_static_names(names)
    }
}

impl fmt::Display for NameParts {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = self.to_string();
        f.write_str(s.as_str())?;
        Ok(())
    }
}

/// A metric identifier.
#[derive(Debug)]
pub struct Key {
    // TODO: once const slicing is possible on stable, we could likely use `beef` for both of these
    name_parts: NameParts,
    labels: Cow<'static, [Label]>,
    hashed: AtomicBool,
    hash: AtomicU64,
}

impl Key {
    /// Creates a [`Key`] from a name.
    pub fn from_name<N>(name: N) -> Self
    where
        N: Into<NameParts>,
    {
        let name_parts = name.into();
        let labels = Cow::owned(Vec::new());

        Self::builder(name_parts, labels)
    }

    /// Creates a [`Key`] from a name and set of labels.
    pub fn from_parts<N, L>(name: N, labels: L) -> Self
    where
        N: Into<NameParts>,
        L: IntoLabels,
    {
        let name_parts = name.into();
        let labels = Cow::owned(labels.into_labels());

        Self::builder(name_parts, labels)
    }

    /// Creates a [`Key`] from a non-static name and a static set of labels.
    pub fn from_static_labels<N>(name: N, labels: &'static [Label]) -> Self
    where
        N: Into<NameParts>,
    {
        Self {
            name_parts: name.into(),
            labels: Cow::<[Label]>::const_slice(labels),
            hashed: AtomicBool::new(false),
            hash: AtomicU64::new(0),
        }
    }

    /// Creates a [`Key`] from a static name.
    ///
    /// This function is `const`, so it can be used in a static context.
    pub const fn from_static_name(name_parts: &'static [SharedString]) -> Self {
        Self::from_static_parts(name_parts, &NO_LABELS)
    }

    /// Creates a [`Key`] from a static name and static set of labels.
    ///
    /// This function is `const`, so it can be used in a static context.
    pub const fn from_static_parts(
        name_parts: &'static [SharedString],
        labels: &'static [Label],
    ) -> Self {
        Self {
            name_parts: NameParts::from_static_names(name_parts),
            labels: Cow::<[Label]>::const_slice(labels),
            hashed: AtomicBool::new(false),
            hash: AtomicU64::new(0),
        }
    }

    fn builder(name_parts: NameParts, labels: Cow<'static, [Label]>) -> Self {
        let hash = generate_key_hash(&name_parts, &labels);

        Self {
            name_parts,
            labels,
            hashed: AtomicBool::new(true),
            hash: AtomicU64::new(hash),
        }
    }

    /// Name parts of this key.
    pub fn name(&self) -> &NameParts {
        &self.name_parts
    }

    /// Labels of this key, if they exist.
    pub fn labels(&self) -> Iter<Label> {
        self.labels.iter()
    }

    /// Appends a part to the name,
    pub fn append_name<S: Into<SharedString>>(self, part: S) -> Self {
        let name_parts = self.name_parts.append(part);
        Self::builder(name_parts, self.labels)
    }

    /// Prepends a part to the name.
    pub fn prepend_name<S: Into<SharedString>>(self, part: S) -> Self {
        let name_parts = self.name_parts.prepend(part);
        Self::builder(name_parts, self.labels)
    }

    /// Consumes this [`Key`], returning the name parts and any labels.
    pub fn into_parts(self) -> (NameParts, Vec<Label>) {
        (self.name_parts.clone(), self.labels.into_owned())
    }

    /// Clones this [`Key`], and expands the existing set of labels.
    pub fn with_extra_labels(&self, extra_labels: Vec<Label>) -> Self {
        if extra_labels.is_empty() {
            return self.clone();
        }

        let name_parts = self.name_parts.clone();
        let mut labels = self.labels.clone().into_owned();
        labels.extend(extra_labels);

        Self::builder(name_parts, labels.into())
    }

    /// Gets the hash value for this key.
    pub fn get_hash(&self) -> u64 {
        if self.hashed.load(Ordering::Acquire) {
            self.hash.load(Ordering::Acquire)
        } else {
            let hash = generate_key_hash(&self.name_parts, &self.labels);
            self.hash.store(hash, Ordering::Release);
            self.hashed.store(true, Ordering::Release);
            hash
        }
    }
}

fn generate_key_hash(name_parts: &NameParts, labels: &Cow<'static, [Label]>) -> u64 {
    let mut hasher = KeyHasher::default();
    key_hasher_impl(&mut hasher, name_parts, labels);
    hasher.finish()
}

fn key_hasher_impl<H: Hasher>(
    state: &mut H,
    name_parts: &NameParts,
    labels: &Cow<'static, [Label]>,
) {
    name_parts.hash(state);
    labels.hash(state);
}

impl Clone for Key {
    fn clone(&self) -> Self {
        Self {
            name_parts: self.name_parts.clone(),
            labels: self.labels.clone(),
            hashed: AtomicBool::new(self.hashed.load(Ordering::Acquire)),
            hash: AtomicU64::new(self.hash.load(Ordering::Acquire)),
        }
    }
}

impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.name_parts == other.name_parts && self.labels == other.labels
    }
}

impl Eq for Key {}

impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Key {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        (&self.name_parts, &self.labels).cmp(&(&other.name_parts, &other.labels))
    }
}

impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        key_hasher_impl(state, &self.name_parts, &self.labels);
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.labels.is_empty() {
            write!(f, "Key({})", self.name_parts)
        } else {
            write!(f, "Key({}, [", self.name_parts)?;
            let mut first = true;
            for label in self.labels.as_ref() {
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
    fn from(name: String) -> Self {
        Self::from_name(name)
    }
}

impl From<&'static str> for Key {
    fn from(name: &'static str) -> Self {
        Self::from_name(name)
    }
}

impl<N, L> From<(N, L)> for Key
where
    N: Into<SharedString>,
    L: IntoLabels,
{
    fn from(parts: (N, L)) -> Self {
        Self::from_parts(NameParts::from_name(parts.0), parts.1)
    }
}

#[cfg(test)]
mod tests {
    use super::Key;
    use crate::{Label, SharedString};
    use std::collections::HashMap;

    static BORROWED_NAME: [SharedString; 1] = [SharedString::const_str("name")];
    static FOOBAR_NAME: [SharedString; 1] = [SharedString::const_str("foobar")];
    static BORROWED_BASIC: Key = Key::from_static_name(&BORROWED_NAME);
    static LABELS: [Label; 1] = [Label::from_static_parts("key", "value")];
    static BORROWED_LABELS: Key = Key::from_static_parts(&BORROWED_NAME, &LABELS);

    #[test]
    fn test_key_ord_and_partialord() {
        let keys_expected: Vec<Key> = vec![
            Key::from_name("aaaa").into(),
            Key::from_name("bbbb").into(),
            Key::from_name("cccc").into(),
        ];

        let keys_unsorted: Vec<Key> = vec![
            Key::from_name("bbbb").into(),
            Key::from_name("cccc").into(),
            Key::from_name("aaaa").into(),
        ];

        let keys = {
            let mut keys = keys_unsorted.clone();
            keys.sort();
            keys
        };
        assert_eq!(keys, keys_expected);

        let keys = {
            let mut keys = keys_unsorted.clone();
            keys.sort_by(|a, b| a.partial_cmp(b).unwrap());
            keys
        };
        assert_eq!(keys, keys_expected);
    }

    #[test]
    fn test_key_eq_and_hash() {
        let mut keys = HashMap::new();

        let owned_basic: Key = Key::from_name("name").into();
        assert_eq!(&owned_basic, &BORROWED_BASIC);

        let previous = keys.insert(owned_basic, 42);
        assert!(previous.is_none());

        let previous = keys.get(&BORROWED_BASIC);
        assert_eq!(previous, Some(&42));

        let labels = LABELS.to_vec();
        let owned_labels = Key::from_parts(&BORROWED_NAME[..], labels);
        assert_eq!(&owned_labels, &BORROWED_LABELS);

        let previous = keys.insert(owned_labels, 43);
        assert!(previous.is_none());

        let previous = keys.get(&BORROWED_LABELS);
        assert_eq!(previous, Some(&43));

        let basic: Key = "constant_key".into();
        let cloned_basic = basic.clone();
        assert_eq!(basic, cloned_basic);
    }

    #[test]
    fn test_key_data_proper_display() {
        let key1 = Key::from_name("foobar");
        let result1 = key1.to_string();
        assert_eq!(result1, "Key(foobar)");

        let key2 = Key::from_parts(&FOOBAR_NAME[..], vec![Label::new("system", "http")]);
        let result2 = key2.to_string();
        assert_eq!(result2, "Key(foobar, [system = http])");

        let key3 = Key::from_parts(
            &FOOBAR_NAME[..],
            vec![Label::new("system", "http"), Label::new("user", "joe")],
        );
        let result3 = key3.to_string();
        assert_eq!(result3, "Key(foobar, [system = http, user = joe])");

        let key4 = Key::from_parts(
            &FOOBAR_NAME[..],
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
}
