use std::{slice::Iter, sync::Arc};

use crate::{cow::Cow, LabelSet, SharedString};

/// Label storage that can be owned, borrowed from a static slice, or shared through an [`Arc`].
///
/// Construct this from an `Arc<[Label]>` to reuse one immutable label array across metric names.
/// Use [`LabelSet`] when label sets also share prefixes.
pub type SharedLabels = Cow<'static, [Label]>;

/// Metadata for a metric key in the form of a key/value pair.
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

impl<K, V> From<(&K, &V)> for Label
where
    K: Into<SharedString> + Clone,
    V: Into<SharedString> + Clone,
{
    fn from(pair: (&K, &V)) -> Label {
        Label::new(pair.0.clone(), pair.1.clone())
    }
}

/// A value that can be converted to a vector of [`Label`]s.
pub trait IntoLabels {
    /// Consumes this value, turning it into a vector of [`Label`]s.
    fn into_labels(self) -> Vec<Label>;

    /// Consumes this value, preserving shared label storage when supported.
    ///
    /// The default implementation uses [`IntoLabels::into_labels`]. Implementations that already
    /// have shared storage can override this method to avoid copying their label array.
    fn into_shared_labels(self) -> SharedLabels
    where
        Self: Sized,
    {
        self.into_labels().into()
    }

    /// Consumes this value, preserving shared arrays and label prefixes when supported.
    ///
    /// The default uses [`IntoLabels::into_shared_labels`], so existing implementations retain
    /// their shared storage without any changes.
    fn into_label_set(self) -> LabelSet
    where
        Self: Sized,
    {
        self.into_shared_labels().into()
    }
}

impl IntoLabels for Arc<[Label]> {
    fn into_labels(self) -> Vec<Label> {
        self.as_ref().to_vec()
    }

    fn into_shared_labels(self) -> SharedLabels {
        self.into()
    }
}

impl IntoLabels for SharedLabels {
    fn into_labels(self) -> Vec<Label> {
        self.into_owned()
    }

    fn into_shared_labels(self) -> SharedLabels {
        self
    }
}

impl IntoLabels for &SharedLabels {
    fn into_labels(self) -> Vec<Label> {
        self.iter().cloned().collect()
    }

    fn into_shared_labels(self) -> SharedLabels {
        self.to_retained()
    }
}

impl IntoLabels for LabelSet {
    fn into_labels(self) -> Vec<Label> {
        self.into_owned()
    }

    fn into_shared_labels(self) -> SharedLabels {
        LabelSet::into_shared_labels(self)
    }

    fn into_label_set(self) -> LabelSet {
        self
    }
}

impl IntoLabels for &LabelSet {
    fn into_shared_labels(self) -> SharedLabels {
        self.to_shared_labels()
    }

    fn into_labels(self) -> Vec<Label> {
        self.iter().cloned().collect()
    }

    fn into_label_set(self) -> LabelSet {
        self.to_retained()
    }
}

impl IntoLabels for crate::Labels<'_> {
    fn into_labels(self) -> Vec<Label> {
        self.cloned().collect()
    }
}

impl IntoLabels for Vec<Label> {
    fn into_labels(self) -> Vec<Label> {
        self
    }
}

impl IntoLabels for Iter<'_, Label> {
    fn into_labels(self) -> Vec<Label> {
        self.cloned().collect()
    }
}

impl<T: ?Sized, L> IntoLabels for &T
where
    Self: IntoIterator<Item = L>,
    L: Into<Label>,
{
    fn into_labels(self) -> Vec<Label> {
        self.into_iter().map(|l| l.into()).collect()
    }
}

#[cfg(test)]
mod label_tests {
    use super::*;

    #[test]
    fn slice_labels() {
        let labels = [("x", "a"), ("y", "b")];

        fn from_slice_to_labels(labels: &[(&'static str, &'static str)]) -> Vec<Label> {
            labels.into_labels()
        }

        let expected = vec![Label::new("x", "a"), Label::new("y", "b")];
        assert_eq!(from_slice_to_labels(&labels), expected);
    }

    #[test]
    fn btreemap_to_labels() {
        use std::collections::BTreeMap;

        let labels_btreemap = BTreeMap::from([("customer", "Rust Foundation")]);

        let expected = vec![Label::new("customer", "Rust Foundation")];
        assert_eq!(labels_btreemap.into_labels(), expected);
    }
}
