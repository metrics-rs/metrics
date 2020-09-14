//! Label filtering.

use metrics::Label;

/// [`LabelFilter`] trait encapsulates the ability to filter labels, i.e.
/// determinig whether a particular label should be included in the key or not.
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
