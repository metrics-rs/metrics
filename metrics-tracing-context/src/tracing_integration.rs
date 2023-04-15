//! The code that integrates with the `tracing` crate.

use lockfree_object_pool::{LinearObjectPool, LinearOwnedReusable};
use metrics::{Key, Label};
use once_cell::sync::OnceCell;
use std::sync::Arc;
use std::{any::TypeId, marker::PhantomData};
use tracing_core::span::{Attributes, Id, Record};
use tracing_core::{field::Visit, Dispatch, Field, Subscriber};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

fn get_pool() -> &'static Arc<LinearObjectPool<Vec<Label>>> {
    static POOL: OnceCell<Arc<LinearObjectPool<Vec<Label>>>> = OnceCell::new();
    POOL.get_or_init(|| Arc::new(LinearObjectPool::new(Vec::new, Vec::clear)))
}
/// Span fields mapped as metrics labels.
///
/// Hidden from documentation as there is no need for end users to ever touch this type, but it must
/// be public in order to be pulled in by external benchmark code.
#[doc(hidden)]
pub struct Labels(pub LinearOwnedReusable<Vec<Label>>);

impl Labels {
    pub(crate) fn extend_from_labels(&mut self, other: &Labels) {
        self.0.extend_from_slice(other.as_ref());
    }
}

impl Default for Labels {
    fn default() -> Self {
        Labels(get_pool().pull_owned())
    }
}

impl Visit for Labels {
    fn record_str(&mut self, field: &Field, value: &str) {
        let label = Label::new(field.name(), value.to_string());
        self.0.push(label);
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        let label = Label::from_static_parts(field.name(), if value { "true" } else { "false" });
        self.0.push(label);
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        let mut buf = itoa::Buffer::new();
        let s = buf.format(value);
        let label = Label::new(field.name(), s.to_string());
        self.0.push(label);
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        let mut buf = itoa::Buffer::new();
        let s = buf.format(value);
        let label = Label::new(field.name(), s.to_string());
        self.0.push(label);
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let value_string = format!("{:?}", value);
        let label = Label::new(field.name(), value_string);
        self.0.push(label);
    }
}

impl Labels {
    fn from_attributes(attrs: &Attributes<'_>) -> Labels {
        let mut labels = Labels::default();
        let record = Record::new(attrs.values());
        record.record(&mut labels);
        labels
    }
}

impl AsRef<[Label]> for Labels {
    fn as_ref(&self) -> &[Label] {
        &self.0
    }
}

pub struct WithContext {
    with_labels: fn(&Dispatch, &Id, f: &mut dyn FnMut(&Labels) -> Option<Key>) -> Option<Key>,
}

impl WithContext {
    pub fn with_labels(
        &self,
        dispatch: &Dispatch,
        id: &Id,
        f: &mut dyn FnMut(&[Label]) -> Option<Key>,
    ) -> Option<Key> {
        let mut ff = |labels: &Labels| f(labels.as_ref());
        (self.with_labels)(dispatch, id, &mut ff)
    }
}

/// [`MetricsLayer`] is a [`tracing_subscriber::Layer`] that captures the span
/// fields and allows them to be later on used as metrics labels.
pub struct MetricsLayer<S> {
    ctx: WithContext,
    _subscriber: PhantomData<fn(S)>,
}

impl<S> MetricsLayer<S>
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    /// Create a new `MetricsLayer`.
    pub fn new() -> Self {
        let ctx = WithContext { with_labels: Self::with_labels };

        Self { ctx, _subscriber: PhantomData }
    }

    fn with_labels(
        dispatch: &Dispatch,
        id: &Id,
        f: &mut dyn FnMut(&Labels) -> Option<Key>,
    ) -> Option<Key> {
        let subscriber = dispatch
            .downcast_ref::<S>()
            .expect("subscriber should downcast to expected type; this is a bug!");
        let span = subscriber.span(id).expect("registry should have a span for the current ID");

        let result =
            if let Some(labels) = span.extensions().get::<Labels>() { f(labels) } else { None };
        result
    }
}

impl<S> Layer<S> for MetricsLayer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, cx: Context<'_, S>) {
        let span = cx.span(id).expect("span must already exist!");
        let mut labels = Labels::from_attributes(attrs);

        if let Some(parent) = span.parent() {
            if let Some(parent_labels) = parent.extensions().get::<Labels>() {
                labels.extend_from_labels(parent_labels);
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

impl<S> Default for MetricsLayer<S>
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    fn default() -> Self {
        MetricsLayer::new()
    }
}
