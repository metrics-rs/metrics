//! Field merge policies.
//!
//! A [`FieldMergePolicy`] determines what happens when the same span field name shows up
//! more than once: either a child span defines a field that its parent also defines, or
//! [`Span::record`][tracing::Span::record] is used to set a value for a field that the span
//! already holds. Policies are configured per field name via
//! [`MetricsLayer::with_field_merge_policy`][crate::MetricsLayer::with_field_merge_policy].

/// The merge policy applied when a value is merged over an existing value for the same
/// span field name.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum FieldMergePolicy {
    /// The most recently defined value wins: a child span's field overrides the value
    /// inherited from its parent, and a recorded value overrides the previously held one.
    ///
    /// This is the historical behavior of the crate, and it applies to any field name
    /// without an explicitly configured policy.
    Override,
    /// Compose both values hierarchically as `<outer><sep><inner>`.
    ///
    /// When a child span inherits a field from its parent, the parent's value is treated
    /// as the outer value and the child's own value as the inner one. When a value is set
    /// via [`Span::record`][tracing::Span::record], the previously held value is treated
    /// as the outer value and the newly recorded value as the inner one. If only one of
    /// the two values exists, it is used as-is.
    ///
    /// As spans nest, values accumulate into a path: given `component = "analyzer"` on the
    /// root span, `component = "worker"` on a child span, and `component = "task"` on a
    /// grandchild span, metrics emitted within the grandchild span see
    /// `component = "analyzer.worker.task"`.
    Append(&'static str),
}
