use std::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
    iter::FusedIterator,
    sync::{Arc, OnceLock},
};

use crate::{cow::Cow, IntoLabels, Label, SharedLabels};

/// Immutable labels that can share complete arrays and combine shared sets.
///
/// Use [`labels!`](crate::labels) or [`with_extra_labels`](Self::with_extra_labels) to extend a set
/// without copying its labels.
/// Iteration visits input sets in order. Repeated label keys are kept; added labels
/// do not replace existing ones.
/// [`with_extra_labels`](Self::with_extra_labels) also combines shared sets without macros.
/// Recorder layers can use [`Key::with_extra_label_set`](crate::Key::with_extra_label_set) to
/// append one shared set to many keys.
///
/// ```
/// use metrics::{counter, labels};
///
/// let base_labels = labels!("label_a" => "value_a");
/// let extended_labels = labels!(&base_labels, "label_b" => "value_b");
/// let nested_labels = labels!(&extended_labels, "label_c" => "value_c");
/// let _ = counter!("counter_a", &extended_labels);
/// let _ = counter!("counter_b", &nested_labels);
/// ```
///
/// Cloning a shared set, retaining a key, and renaming a key reuse its storage. Slice access through
/// [`LabelSet::as_slice`] or [`Labels::as_slice`] builds and caches a flat copy for an extended set. Iteration
/// and key hashing do not need that copy. Chains deeper than 32 extensions are flattened to bound
/// lookup and drop costs.
#[derive(Clone)]
pub struct LabelSet(Storage);

#[derive(Clone)]
enum Storage {
    Flat(Cow<'static, [Label]>),
    Prefixed(Arc<Prefix>),
}

struct Prefix {
    prefix: LabelSet,
    extra: LabelSet,
    len: usize,
    hash: u64,
    depth: usize,
    flat: OnceLock<Vec<Label>>,
}

impl LabelSet {
    /// Creates a label set from any existing label input, preserving its shared storage.
    pub fn new<L: IntoLabels>(labels: L) -> Self {
        labels.into_labels()
    }

    /// Borrows a static label slice without allocation.
    pub const fn const_slice(labels: &'static [Label]) -> Self {
        Self(Storage::Flat(Cow::const_slice(labels)))
    }

    /// Stores an owned array. Cloning this storage copies its labels until it is retained.
    pub fn from_owned(labels: Vec<Label>) -> Self {
        Self(Storage::Flat(Cow::from_owned(labels)))
    }

    /// Shares an immutable label array without copying it.
    pub fn from_shared(labels: Arc<[Label]>) -> Self {
        Self(Storage::Flat(Cow::from_shared(labels)))
    }

    /// Borrows a static label slice without allocation.
    pub fn from_borrowed(labels: &'static [Label]) -> Self {
        Self::const_slice(labels)
    }

    /// Returns storage that is cheap to clone, copying owned storage once if needed.
    pub fn to_retained(&self) -> Self {
        match &self.0 {
            Storage::Flat(labels) => Self(Storage::Flat(labels.to_retained())),
            Storage::Prefixed(_) => self.clone(),
        }
    }

    /// Extends this set, preserving shared storage in both inputs.
    ///
    /// An empty extension returns retained storage for the same set. Owned storage created with
    /// [`from_owned`](Self::from_owned) is copied once for each call; retain it first when making
    /// several child sets. [`From<Vec<Label>>`] creates shared storage directly.
    /// An additional [`LabelSet`] can itself contain several shared sets; its storage is reused.
    pub fn with_extra_labels<L: IntoLabels>(&self, extra: L) -> Self {
        let extra = extra.into_labels();
        if extra.is_empty() {
            return self.to_retained();
        }
        if self.is_empty() {
            return extra.into_retained();
        }
        let prefix = if self.depth() >= 32 {
            Self::from(self.iter().cloned().collect::<Vec<_>>())
        } else {
            self.to_retained()
        };
        let extra = if extra.depth() >= 32 { Self::from(extra.into_owned()) } else { extra };
        let len = prefix.len().checked_add(extra.len()).expect("too many metric labels");
        let hash = prefix.hash_sum().wrapping_add(extra.hash_sum());
        let depth = prefix.depth().max(extra.depth()) + 1;
        Self(Storage::Prefixed(Arc::new(Prefix {
            prefix,
            extra,
            len,
            hash,
            depth,
            flat: OnceLock::new(),
        })))
    }

    /// Number of labels, including the prefix.
    pub fn len(&self) -> usize {
        match &self.0 {
            Storage::Flat(labels) => labels.len(),
            Storage::Prefixed(labels) => labels.len,
        }
    }

    /// Whether this set has no labels.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Iterates over labels without flattening shared prefixes.
    #[inline]
    pub fn iter(&self) -> Labels<'_> {
        Labels {
            labels: self,
            front: 0,
            back: self.len(),
            forward: Cursor::new(self),
            reverse: Cursor::new(self),
        }
    }

    /// Consumes this set, copying shared or static labels into an owned vector.
    pub fn into_owned(self) -> Vec<Label> {
        match self.0 {
            Storage::Flat(labels) => labels.into_owned(),
            Storage::Prefixed(labels) => {
                labels.prefix.iter().chain(labels.extra.iter()).cloned().collect()
            }
        }
    }

    fn depth(&self) -> usize {
        match &self.0 {
            Storage::Flat(_) => 0,
            Storage::Prefixed(labels) => labels.depth,
        }
    }

    /// Consumes this set, moving owned labels into storage that is cheap to clone.
    pub fn into_retained(self) -> Self {
        match self.0 {
            Storage::Flat(labels) => Self(Storage::Flat(labels.into_retained())),
            Storage::Prefixed(_) => self,
        }
    }

    pub(crate) fn hash_sum(&self) -> u64 {
        match &self.0 {
            Storage::Flat(labels) => crate::key::hash_labels(labels.as_const_slice()),
            Storage::Prefixed(labels) => labels.hash,
        }
    }

    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Storage::Flat(a), Storage::Flat(b)) => std::ptr::eq(a.as_ref(), b.as_ref()),
            (Storage::Prefixed(a), Storage::Prefixed(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl LabelSet {
    /// Returns a slice, building and caching a flat copy for combined sets.
    pub fn as_slice(&self) -> &[Label] {
        match &self.0 {
            Storage::Flat(labels) => labels,
            Storage::Prefixed(labels) => labels.flat.get_or_init(|| self.iter().cloned().collect()),
        }
    }

    // Inspect existing flat storage without populating a combined set's cache.
    #[inline]
    pub(crate) fn as_flat_slice(&self) -> Option<&[Label]> {
        match &self.0 {
            Storage::Flat(labels) => Some(labels),
            Storage::Prefixed(_) => None,
        }
    }
}

impl From<Vec<Label>> for LabelSet {
    fn from(labels: Vec<Label>) -> Self {
        Self::from_shared(labels.into())
    }
}

impl From<SharedLabels> for LabelSet {
    fn from(labels: SharedLabels) -> Self {
        Self(Storage::Flat(labels))
    }
}

impl From<Arc<[Label]>> for LabelSet {
    fn from(labels: Arc<[Label]>) -> Self {
        Self::from_shared(labels)
    }
}

impl From<&'static [Label]> for LabelSet {
    fn from(labels: &'static [Label]) -> Self {
        Self::const_slice(labels)
    }
}

impl Default for LabelSet {
    fn default() -> Self {
        Self::const_slice(&[])
    }
}

/// Converts to flat storage, copying labels when the set is shared or combined.
impl From<LabelSet> for std::borrow::Cow<'static, [Label]> {
    fn from(labels: LabelSet) -> Self {
        match labels.0 {
            Storage::Flat(labels) => labels.into(),
            Storage::Prefixed(_) => Self::Owned(labels.into_owned()),
        }
    }
}

impl fmt::Debug for LabelSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl PartialEq for LabelSet {
    fn eq(&self, other: &Self) -> bool {
        self.ptr_eq(other) || self.iter().eq(other.iter())
    }
}

impl Eq for LabelSet {}

impl PartialOrd for LabelSet {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LabelSet {
    fn cmp(&self, other: &Self) -> Ordering {
        self.iter().cmp(other.iter())
    }
}

impl Hash for LabelSet {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.len().hash(state);
        for label in self.iter() {
            label.hash(state);
        }
    }
}

// Depth is bounded to 32 by with_extra_labels. Each cursor stores at most
// one pending sibling per level, plus the root. Flat inputs only use the leaf.
// Keep the bounded tree stack inline so iteration never needs a heap allocation.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
enum Cursor<'a> {
    Flat(std::slice::Iter<'a, Label>),
    Tree { pending: [Option<&'a LabelSet>; 33], len: usize, leaf: std::slice::Iter<'a, Label> },
}

impl<'a> Cursor<'a> {
    #[inline]
    fn new(labels: &'a LabelSet) -> Self {
        match &labels.0 {
            Storage::Flat(labels) => Self::Flat(labels.iter()),
            Storage::Prefixed(_) => {
                let mut pending = [None; 33];
                pending[0] = Some(labels);
                Self::Tree { pending, len: 1, leaf: [].iter() }
            }
        }
    }

    #[inline]
    fn nth(&mut self, mut skip: usize, reverse: bool) -> Option<&'a Label> {
        let (pending, len, leaf) = match self {
            Self::Flat(leaf) => return if reverse { leaf.nth_back(skip) } else { leaf.nth(skip) },
            Self::Tree { pending, len, leaf } => (pending, len, leaf),
        };
        loop {
            if skip < leaf.len() {
                return if reverse { leaf.nth_back(skip) } else { leaf.nth(skip) };
            }
            skip -= leaf.len();
            *leaf = [].iter();
            if *len == 0 {
                return None;
            }
            *len -= 1;
            let labels = pending[*len].take().unwrap();
            if skip >= labels.len() {
                skip -= labels.len();
                continue;
            }
            match &labels.0 {
                Storage::Flat(labels) => *leaf = labels.iter(),
                Storage::Prefixed(node) => {
                    let (first, second) = if reverse {
                        (&node.extra, &node.prefix)
                    } else {
                        (&node.prefix, &node.extra)
                    };
                    pending[*len] = Some(second);
                    pending[*len + 1] = Some(first);
                    *len += 2;
                }
            }
        }
    }
}

/// An iterator over a key's labels, including any shared prefix.
#[derive(Clone, Debug)]
pub struct Labels<'a> {
    labels: &'a LabelSet,
    front: usize,
    back: usize,
    forward: Cursor<'a>,
    reverse: Cursor<'a>,
}

impl<'a> Labels<'a> {
    /// Returns the remaining labels as a slice, caching a flat copy if this set has a prefix.
    pub fn as_slice(&self) -> &'a [Label] {
        &self.labels.as_slice()[self.front..self.back]
    }
}

impl<'a> Iterator for Labels<'a> {
    type Item = &'a Label;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let label = self.forward.nth(0, false).expect("remaining label");
        self.front += 1;
        Some(label)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.back - self.front;
        (remaining, Some(remaining))
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.len() {
            self.front = self.back;
            return None;
        }
        self.front += n + 1;
        self.forward.nth(n, false)
    }

    fn last(mut self) -> Option<Self::Item> {
        self.nth_back(0)
    }

    fn count(self) -> usize {
        self.len()
    }
}

impl DoubleEndedIterator for Labels<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        self.reverse.nth(0, true)
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.len() {
            self.front = self.back;
            return None;
        }
        self.back -= n + 1;
        self.reverse.nth(n, true)
    }
}

impl ExactSizeIterator for Labels<'_> {}
impl FusedIterator for Labels<'_> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Key;

    /// Combining sets retains the original labels on both sides without flattening either set.
    #[test]
    fn extensions_share_both_label_sets() {
        let first = crate::labels!("label_a" => String::from("value_a"));
        let second = crate::labels!("label_b" => String::from("value_b"));
        let extra = crate::labels!(&second, "label_c" => "value_c");
        let combined = first.with_extra_labels(&extra);
        assert!(std::ptr::eq(first.iter().next().unwrap(), combined.iter().next().unwrap()));
        assert!(std::ptr::eq(second.iter().next().unwrap(), combined.iter().nth(1).unwrap()));
        assert!(std::ptr::eq(extra.iter().nth(1).unwrap(), combined.iter().nth(2).unwrap()));
        assert_unflattened(&combined);
        assert_unflattened(&extra);
    }

    /// Both left- and right-nested combinations keep order and bound their depth.
    #[test]
    fn nested_shared_sets_preserve_iteration_and_hashes() {
        let mut left = LabelSet::default();
        let mut right = LabelSet::default();
        let mut flat = Vec::new();
        for index in 0..100 {
            let part = crate::labels!(index.to_string() => String::from("value"));
            flat.extend(part.iter().cloned());
            left = left.with_extra_labels(&part);
            right = part.with_extra_labels(right);
            assert!(left.depth() <= 32);
            assert!(right.depth() <= 32);
        }
        assert!(left.iter().eq(flat.iter()));
        assert!(right.iter().eq(flat.iter().rev()));
        assert!(right.iter().rev().eq(left.iter()));
        let expected = Key::from_parts("metric_a", flat);
        for labels in [left, right] {
            let key = Key::from_parts("metric_a", &labels);
            assert_eq!(key, expected);
            assert_eq!(key.get_hash(), expected.get_hash());
            assert_eq!(key.cmp(&expected), Ordering::Equal);
            assert_unflattened(&labels);
        }
    }

    /// Joining two nonflat sets keeps both branches through retention, slicing, and extraction.
    #[test]
    fn shared_branches_survive_key_operations() {
        let first = crate::labels!("label_a" => "value_a");
        let left = crate::labels!(&first, "label_b" => "value_b");
        let second = crate::labels!("label_c" => "value_c");
        let right = crate::labels!(&second, "label_d" => "value_d");
        let combined = crate::labels!(&left, &right, "label_e" => "value_e",);
        let expected: Vec<_> = left
            .iter()
            .chain(right.iter())
            .cloned()
            .chain([Label::new("label_e", "value_e")])
            .collect();
        let key = Key::from_parts("metric_a", &left).with_extra_label_set(&right);
        for (actual, source) in key.labels().zip(left.iter().chain(right.iter())) {
            assert!(std::ptr::eq(actual, source));
        }
        let retained = Key::from_parts("metric_a", &combined).to_retained();
        assert_eq!(retained, Key::from_parts("metric_a", expected.clone()));
        assert_unflattened(&combined);
        assert_eq!(retained.labels().as_slice(), expected.as_slice());
        assert_eq!(retained.clone().into_parts().1, expected);
        assert_eq!(combined.iter().next_back().unwrap().key(), "label_e");
        let extra: Vec<_> = right.iter().cloned().collect();
        let legacy =
            Key::from_parts("metric_a", &left).with_extra_labels(extra.into_iter().collect());
        assert_eq!(key, legacy);
    }

    /// Consuming an owned extension of an empty set shares it without cloning string buffers.
    #[test]
    fn empty_prefix_moves_owned_labels_into_shared_storage() {
        let value = String::from("value_a");
        let value_ptr = value.as_ptr();
        let labels = LabelSet::default().with_extra_labels(vec![Label::new("label_a", value)]);
        assert_eq!(labels.iter().next().unwrap().value().as_ptr(), value_ptr);
        assert!(std::ptr::eq(labels.iter().next().unwrap(), labels.clone().iter().next().unwrap()));
        let empty = LabelSet::default();
        let same = labels.with_extra_labels(&empty);
        assert!(std::ptr::eq(labels.iter().next().unwrap(), same.iter().next().unwrap()));
    }

    /// The macro shares static and dynamic labels across clones and retained collections.
    #[test]
    fn labels_macro_creates_reusable_sets() {
        assert!(crate::labels!().is_empty());
        let literals = crate::labels!("label_a" => "value_a", "label_b" => "value_b",);
        assert_eq!(literals.iter().len(), 2);
        assert_eq!(literals.iter().next().unwrap(), &Label::new("label_a", "value_a"));
        assert!(std::ptr::eq(
            literals.iter().next().unwrap(),
            literals.clone().iter().next().unwrap()
        ));

        let evaluations = std::cell::Cell::new(0);
        let dynamic = crate::labels!(
            String::from("label_a") => {
                evaluations.set(evaluations.get() + 1);
                String::from("value_a")
            },
        );
        assert_eq!(evaluations.get(), 1);
        let retained = crate::labels!(&dynamic,);
        let cloned = dynamic.clone();
        assert!(std::ptr::eq(dynamic.iter().next().unwrap(), retained.iter().next().unwrap()));
        assert!(std::ptr::eq(dynamic.iter().next().unwrap(), cloned.iter().next().unwrap()));
        let from_collection = crate::labels!(&[("label_a", "value_a")]);
        assert_eq!(from_collection, dynamic);
    }

    /// Macro extensions reuse ancestor storage and evaluate the prefix only once.
    #[test]
    fn labels_macro_extends_reusable_sets() {
        let base = crate::labels!("label_a" => "value_a");
        let evaluations = std::cell::Cell::new(0);
        let extended = crate::labels!(
            { evaluations.set(evaluations.get() + 1); &base },
            "label_b" => String::from("value_b"),
        );
        let nested = crate::labels!(&extended, "label_c" => "value_c");
        assert_eq!(evaluations.get(), 1);
        assert_eq!(base.len(), 1);
        assert_eq!(extended.len(), 2);
        assert_eq!(nested.len(), 3);
        assert!(std::ptr::eq(base.iter().next().unwrap(), nested.iter().next().unwrap()));
        assert!(std::ptr::eq(extended.iter().nth(1).unwrap(), nested.iter().nth(1).unwrap()));
        let expected = Key::from_parts(
            "metric_a",
            &[("label_c", "value_c"), ("label_b", "value_b"), ("label_a", "value_a")],
        );
        let key = Key::from_parts("metric_a", &nested);
        assert_eq!(key, expected);
        assert_eq!(key.get_hash(), expected.get_hash());
        assert_unflattened(&nested);
        drop(base);
        drop(extended);
        assert_eq!(nested.iter().next().unwrap().value(), "value_a");
    }

    /// Key operations share ancestor labels and do not populate the flat cache.
    #[test]
    fn key_operations_keep_prefixes_shared() {
        let base: Arc<[Label]> = vec![Label::new("label_a", String::from("value_a"))].into();
        let weak = Arc::downgrade(&base);
        let base_labels = LabelSet::from(Arc::clone(&base));
        let extended_labels = base_labels.with_extra_labels(&[("label_b", "value_b")]);
        let nested_labels = extended_labels.with_extra_labels(&[("label_c", "value_c")]);
        let sibling = extended_labels.with_extra_labels(&[("label_c", "value_d")]);
        assert!(std::ptr::eq(nested_labels.iter().next().unwrap(), &base[0]));
        assert!(std::ptr::eq(
            nested_labels.iter().nth(1).unwrap(),
            extended_labels.iter().nth(1).unwrap()
        ));
        assert!(std::ptr::eq(
            sibling.iter().nth(1).unwrap(),
            extended_labels.iter().nth(1).unwrap()
        ));

        let key = Key::from_parts("metric_a", &nested_labels);
        let flat = Key::from_parts("metric_a", nested_labels.iter().cloned().collect::<Vec<_>>());
        assert_eq!(key, flat);
        assert_eq!(key.cmp(&flat), Ordering::Equal);
        assert_eq!(key.get_hash(), flat.get_hash());
        assert_eq!(key.to_string(), flat.to_string());
        let clones = [key.clone(), key.to_retained(), key.with_name("metric_b")];
        for clone in &clones {
            assert!(std::ptr::eq(clone.labels().next().unwrap(), &base[0]));
        }
        assert_unflattened(&nested_labels);
        assert_unflattened(&sibling);

        // Explicit slice access is the only iterator path that creates a flat copy.
        let legacy = key.labels();
        assert_eq!(legacy.as_slice(), flat.labels().as_slice());

        drop(base);
        drop(base_labels);
        drop(extended_labels);
        drop(nested_labels);
        drop(sibling);
        drop(key);
        assert!(weak.upgrade().is_some());
        drop(clones);
        assert!(weak.upgrade().is_none());
    }

    /// Slice access caches a flat array and respects partially consumed iterators.
    #[test]
    fn slice_access_is_lazy_and_shared() {
        let base = LabelSet::from(vec![Label::new("a", "1")]);
        let labels = base.with_extra_labels(&[("b", "2"), ("c", "3"), ("d", "4")]);
        let cloned = labels.clone();
        assert_unflattened(&labels);
        let mut iter = labels.iter();
        assert_eq!(iter.len(), 4);
        assert_eq!(iter.next().unwrap().key(), "a");
        assert_eq!(iter.next_back().unwrap().key(), "d");
        assert_eq!(iter.size_hint(), (2, Some(2)));
        assert_eq!(iter.as_slice(), &[Label::new("b", "2"), Label::new("c", "3")]);
        assert_eq!(labels.as_slice().as_ptr(), cloned.as_slice().as_ptr());
        assert_eq!(iter.next_back().unwrap().key(), "c");
        assert_eq!(iter.next().unwrap().key(), "b");
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next_back(), None);
        assert!(iter.as_slice().is_empty());
        assert_eq!(labels.iter().nth_back(2).unwrap().key(), "b");
        assert_eq!(labels.iter().nth(2).unwrap().key(), "c");
        let mut exhausted = labels.iter();
        assert_eq!(exhausted.nth(usize::MAX), None);
        assert_eq!(exhausted.next_back(), None);
        assert_eq!(labels.iter().nth_back(usize::MAX), None);
        assert_eq!(&labels.as_slice()[1..3], &[Label::new("b", "2"), Label::new("c", "3")]);
        let borrowed = labels.as_slice();
        assert_eq!(borrowed, labels.as_slice());
        let cow: std::borrow::Cow<'static, [Label]> = labels.clone().into();
        assert_eq!(cow.as_ref(), borrowed);
    }

    /// Empty extensions, owned parents, and repeated extensions preserve their label contents.
    #[test]
    fn extensions_handle_empty_owned_and_deep_sets() {
        let empty = LabelSet::default();
        assert!(empty.with_extra_labels(Vec::new()).is_empty());
        let owned = LabelSet::from_owned(vec![Label::new("a", String::from("1"))]);
        let retained = owned.with_extra_labels(Vec::new());
        let mut labels = retained.clone();
        drop(owned);
        for i in 0..100 {
            labels = labels.with_extra_labels(vec![Label::new(i.to_string(), "value")]);
            assert!(labels.depth() <= 32);
        }
        assert_eq!(labels.len(), 101);
        assert_eq!(labels.iter().next(), retained.iter().next());
        assert_eq!(labels.iter().next_back().unwrap().key(), "99");
        let flat = Key::from_parts("metric_a", labels.iter().cloned().collect::<Vec<_>>());
        let key = Key::from_parts("metric_a", &labels);
        assert_eq!(key, flat);
        assert_eq!(key.get_hash(), flat.get_hash());
        let (_, mut extracted) = key.into_parts();
        extracted[0] = Label::new("changed", "value");
        assert_eq!(labels.iter().next().unwrap().key(), "a");
    }

    #[test]
    fn consuming_macros_move_owned_string_buffers() {
        for form in 0..3 {
            let value = String::from("owned value");
            let ptr = value.as_ptr();
            let owned = LabelSet::from_owned(vec![Label::new("a", value)]);
            let labels = match form {
                0 => crate::labels!(owned),
                1 => crate::labels!(owned, "b" => "2"),
                _ => crate::labels!(owned, crate::labels!("b" => "2"), "c" => "3"),
            };
            assert_eq!(labels.iter().next().unwrap().value().as_ptr(), ptr);
            assert!(labels.ptr_eq(&labels.clone()));
        }
    }

    #[test]
    fn label_conversions_preserve_storage_shape() {
        static STATIC: [Label; 1] = [Label::from_static_parts("a", "1")];
        let shared: Arc<[Label]> = vec![Label::new("b", "2")].into();
        for labels in [LabelSet::const_slice(&STATIC), LabelSet::from_shared(shared)] {
            let borrowed = (&labels).into_labels();
            assert!(labels.ptr_eq(&borrowed));
            let consumed = labels.into_labels();
            assert!(borrowed.ptr_eq(&consumed));
        }
        let value = String::from("owned value");
        let ptr = value.as_ptr();
        let owned = vec![Label::new("a", value)];
        let array_ptr = owned.as_ptr();
        let consumed = owned.into_labels();
        assert_eq!(consumed.iter().next().unwrap().value().as_ptr(), ptr);
        assert_eq!(consumed.as_slice().as_ptr(), array_ptr);

        let combined = crate::labels!("a" => "1").with_extra_labels(crate::labels!("b" => "2"));
        let borrowed = (&combined).into_labels();
        assert!(combined.ptr_eq(&borrowed));
        assert_unflattened(&combined);
        let consumed = combined.into_labels();
        assert!(borrowed.ptr_eq(&consumed));
        assert_unflattened(&consumed);
    }

    #[test]
    fn mixed_tree_iteration_matches_slice_iteration() {
        // Exercise every forward/backward schedule across differently shaped trees.
        for right_nested in [false, true] {
            let mut labels = LabelSet::default();
            for i in 0..8 {
                let leaf = crate::labels!(i.to_string() => "value");
                labels = if right_nested {
                    leaf.with_extra_labels(labels)
                } else {
                    labels.with_extra_labels(leaf)
                };
            }
            let expected: Vec<_> = labels.iter().cloned().collect();
            for mask in 0..256 {
                let mut actual = labels.iter();
                let mut flat = expected.iter();
                for bit in 0..8 {
                    if mask & (1 << bit) == 0 {
                        assert_eq!(actual.next(), flat.next());
                    } else {
                        assert_eq!(actual.next_back(), flat.next_back());
                    }
                    assert_eq!(actual.len(), flat.len());
                    assert!(actual.clone().eq(flat.clone()));
                }
                assert_eq!(actual.next(), None);
                assert_eq!(actual.next_back(), None);
            }
            assert_unflattened(&labels);
        }
    }

    #[test]
    fn skips_bypass_shared_subtrees() {
        let mut labels = crate::labels!("a" => "1");
        // A billion logical labels with only 30 shared composition nodes.
        for _ in 0..30 {
            labels = labels.with_extra_labels(&labels);
        }
        let mut iter = labels.iter();
        assert_eq!(iter.nth(usize::MAX), None);
        assert_eq!(iter.next_back(), None);
        assert_eq!(labels.iter().nth_back(usize::MAX), None);
        let mut iter = labels.iter();
        assert_eq!(iter.nth((1 << 29) - 1), Some(&Label::new("a", "1")));
        assert_eq!(iter.nth_back((1 << 29) - 1), Some(&Label::new("a", "1")));
        assert_eq!(iter.len(), 0);
        // Don't flatten this exponentially shared graph for an assertion.
    }

    fn assert_unflattened(labels: &LabelSet) {
        if let Storage::Prefixed(node) = &labels.0 {
            assert!(node.flat.get().is_none());
            assert_unflattened(&node.prefix);
            assert_unflattened(&node.extra);
        }
    }
}
