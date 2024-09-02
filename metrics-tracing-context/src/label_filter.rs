//! Label filtering.

use std::collections::HashSet;

use metrics::{KeyName, Label};

/// [`LabelFilter`] trait encapsulates the ability to filter labels, i.e.
/// determining whether a particular span field should be included as a label or not.
pub trait LabelFilter {
    /// Returns `true` if the passed `label` of the metric named `name` should
    /// be included in the key.
    fn should_include_label(&self, name: &KeyName, label: &Label) -> bool;
}

/// A [`LabelFilter`] that allows all labels.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct IncludeAll;

impl LabelFilter for IncludeAll {
    fn should_include_label(&self, _name: &KeyName, _label: &Label) -> bool {
        true
    }
}

/// A [`LabelFilter`] that only allows labels contained in a predefined list.
#[derive(Debug, Clone)]
pub struct Allowlist {
    /// The set of allowed label names.
    label_names: HashSet<String>,
}

impl Allowlist {
    /// Create a [`Allowlist`] filter with the provided label names.
    pub fn new<I, S>(allowed: I) -> Allowlist
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self { label_names: allowed.into_iter().map(|s| s.as_ref().to_string()).collect() }
    }
}

impl LabelFilter for Allowlist {
    fn should_include_label(&self, _name: &KeyName, label: &Label) -> bool {
        self.label_names.contains(label.key())
    }
}
