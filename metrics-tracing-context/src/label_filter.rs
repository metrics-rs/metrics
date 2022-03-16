//! Label filtering.

use std::collections::HashSet;

use metrics::Label;

/// [`LabelFilter`] trait encapsulates the ability to filter labels, i.e.
/// determining whether a particular span field should be included as a label or not.
pub trait LabelFilter {
    /// Returns `true` if the passed label should be included in the key.
    fn should_include_label(&self, label: &Label) -> bool;
}

/// A [`LabelFilter`] that allows all labels.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct IncludeAll;

impl LabelFilter for IncludeAll {
    fn should_include_label(&self, _label: &Label) -> bool {
        true
    }
}

/// A [`LabelFilter`] that allows an allowed list of label names.
#[derive(Debug, Clone)]
pub struct Allowlist {
    /// The set of allowed label names.
    label_names: HashSet<String>,
}

impl Allowlist {
    /// Create a [`Allowlist`] filter with the provide label names.
    pub fn new(label_names: &[&str]) -> Allowlist {
        let set: HashSet<String> = label_names.iter().map(|l| l.to_string()).collect();
        Allowlist { label_names: set }
    }
}

impl LabelFilter for Allowlist {
    fn should_include_label(&self, label: &Label) -> bool {
        self.label_names.contains(label.key())
    }
}
