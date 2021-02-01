//! The code that integrates with the `tracing` crate.

use metrics::Label;
use smallvec::SmallVec;
use std::{any::TypeId, marker::PhantomData};
use tracing_core::span::{Attributes, Id, Record};
use tracing_core::{field::Visit, Dispatch, Field, Subscriber};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

use crate::{label_filter, LabelFilter};

/// Per-span extension for collecting labels from fields.
///
/// Hidden from documentation as there is no need for end users to ever touch this type, but it must
/// be public in order to be pulled in by external benchmark code.
#[doc(hidden)]
pub struct Labels(pub Inner);
type Inner = SmallVec<[Label; 16]>;

/// A visitor over the labels and a filter for collecting labels from fields.
///
/// Hidden from documentation as there is no need for end users to ever touch this type, but it must
/// be public in order to be pulled in by external benchmark code.
#[doc(hidden)]
pub struct LabelsVisitor<'a, F: LabelFilter> {
    pub labels: &'a mut Labels,
    pub label_filter: &'a F,
}

impl<'a, F: LabelFilter> LabelsVisitor<'a, F> {
    fn push_if_allowed(&mut self, label: Label) {
        if self.label_filter.should_include_label(&label) {
            self.labels.0.push(label);
        }
    }
}

impl<'a, F: LabelFilter> Visit for LabelsVisitor<'a, F> {
    fn record_str(&mut self, field: &Field, value: &str) {
        let label = Label::new(field.name(), value.to_owned());
        self.push_if_allowed(label)
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        let label = Label::from_static_parts(field.name(), if value { "true" } else { "false" });
        self.push_if_allowed(label)
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        // Maximum length is 20 characters but 32 is a nice power-of-two number.
        let mut s = String::with_capacity(32);
        itoa::fmt(&mut s, value).expect("failed to format/write i64");
        let label = Label::new(field.name(), s);
        self.push_if_allowed(label)
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        // Maximum length is 20 characters but 32 is a nice power-of-two number.
        let mut s = String::with_capacity(32);
        itoa::fmt(&mut s, value).expect("failed to format/write u64");
        let label = Label::new(field.name(), s);
        self.push_if_allowed(label)
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let value_string = format!("{:?}", value);
        let label = Label::new(field.name(), value_string);
        self.push_if_allowed(label)
    }
}

impl Labels {
    fn from_attributes<F>(attrs: &Attributes<'_>, label_filter: &F) -> Self
    where
        F: LabelFilter,
    {
        let mut labels = Self(SmallVec::new()); // TODO: with_capacity?
        let mut labels_visitor = LabelsVisitor {
            labels: &mut labels,
            label_filter,
        };
        let record = Record::new(attrs.values());
        record.record(&mut labels_visitor);
        drop(labels_visitor);
        labels
    }
}

impl AsRef<Inner> for Labels {
    fn as_ref(&self) -> &Inner {
        &self.0
    }
}

pub struct WithContext {
    with_labels: fn(&Dispatch, &Id, f: &mut dyn FnMut(&Labels)),
}

impl WithContext {
    pub fn with_labels<'a>(&self, dispatch: &'a Dispatch, id: &Id, f: &mut dyn FnMut(&Inner)) {
        let mut ff = |labels: &Labels| f(labels.as_ref());
        (self.with_labels)(dispatch, id, &mut ff)
    }
}

/// [`MetricsLayer`] is a [`tracing_subscriber::Layer`] that captures the span
/// fields and allows them to be later on used as metrics labels.
pub struct MetricsLayer<S, F> {
    ctx: WithContext,
    label_filter: F,
    _subscriber: PhantomData<fn(S)>,
    _priv: (),
}

impl<S> MetricsLayer<S, label_filter::IncludeAll>
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    /// Creates a new [`MetricsLayer`].
    pub fn all() -> Self {
        Self::new(label_filter::IncludeAll)
    }
}

impl<S, F> MetricsLayer<S, F>
where
    S: Subscriber + for<'span> LookupSpan<'span>,
    F: LabelFilter + 'static,
{
    /// Create a new `MetricsLayer`.
    pub fn new(label_filter: F) -> Self {
        let ctx = WithContext {
            with_labels: Self::with_labels,
        };

        Self {
            ctx,
            label_filter,
            _subscriber: PhantomData,
            _priv: (),
        }
    }

    fn with_labels(dispatch: &Dispatch, id: &Id, f: &mut dyn FnMut(&Labels)) {
        let span = {
            let subscriber = dispatch
                .downcast_ref::<S>()
                .expect("subscriber should downcast to expected type; this is a bug!");
            subscriber
                .span(id)
                .expect("registry should have a span for the current ID")
        };

        let extensions = span.extensions();
        if let Some(value) = extensions.get::<Labels>() {
            f(value);
        }
    }
}

impl<S, F> Layer<S> for MetricsLayer<S, F>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    F: LabelFilter + 'static,
{
    fn new_span(&self, attrs: &Attributes<'_>, id: &Id, cx: Context<'_, S>) {
        let span = cx.span(id).expect("span must already exist!");

        // Get labels for this span.
        let mut labels = Labels::from_attributes(attrs, &self.label_filter);

        // Add labels from the parent spans.
        let parent_spans = span.parents();
        for parent_span in parent_spans {
            let extensions = parent_span.extensions();
            if let Some(value) = extensions.get::<Labels>() {
                labels.0.extend(value.0.iter().cloned());
            }
        }

        span.extensions_mut().insert(labels);
    }

    unsafe fn downcast_raw(&self, id: TypeId) -> Option<*const ()> {
        match id {
            id if id == TypeId::of::<Self>() => Some(self as *const _ as *const ()),
            id if id == TypeId::of::<WithContext>() => Some(&self.ctx as *const _ as *const ()),
            _ => None,
        }
    }
}

impl<S> Default for MetricsLayer<S, label_filter::IncludeAll>
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    fn default() -> Self {
        MetricsLayer::all()
    }
}

/// An extention to the `tracing::Span`, enabling the access to labels.
pub trait SpanExt {
    /// Run the provided function with a read-only access to labels.
    fn with_labels<F>(&self, f: F)
    where
        F: FnMut(&Inner);
}

impl SpanExt for tracing::Span {
    fn with_labels<F>(&self, mut f: F)
    where
        F: FnMut(&Inner),
    {
        self.with_subscriber(|(id, subscriber)| {
            if let Some(ctx) = subscriber.downcast_ref::<WithContext>() {
                ctx.with_labels(subscriber, id, &mut f)
            }
        });
    }
}
