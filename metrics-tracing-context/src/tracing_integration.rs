//! The code that integrates with the `tracing` crate.

use indexmap::IndexMap;
use lockfree_object_pool::{LinearObjectPool, LinearOwnedReusable};
use metrics::{Key, SharedString};
use once_cell::sync::OnceCell;
use std::sync::Arc;
use std::{any::TypeId, cmp, marker::PhantomData};
use tracing_core::span::{Attributes, Id, Record};
use tracing_core::{field::Visit, Dispatch, Field, Subscriber};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

pub(crate) type Map = IndexMap<SharedString, SharedString>;

fn get_pool() -> &'static Arc<LinearObjectPool<Map>> {
    static POOL: OnceCell<Arc<LinearObjectPool<Map>>> = OnceCell::new();
    POOL.get_or_init(|| Arc::new(LinearObjectPool::new(Map::new, Map::clear)))
}

/// Span fields mapped as metrics labels.
///
/// Hidden from documentation as there is no need for end users to ever touch this type, but it must
/// be public in order to be pulled in by external benchmark code.
#[doc(hidden)]
pub struct Labels(pub LinearOwnedReusable<Map>);

impl Labels {
    pub(crate) fn extend_from_labels(&mut self, other: &Labels) {
        let new_len = cmp::max(self.as_ref().len(), other.as_ref().len());
        let additional = new_len - self.as_ref().len();
        self.0.reserve(additional);
        for (k, v) in other.as_ref() {
            self.0.insert(k.clone(), v.clone());
        }
    }
}

impl Default for Labels {
    fn default() -> Self {
        Labels(get_pool().pull_owned())
    }
}

impl Visit for Labels {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().into(), value.to_owned().into());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().into(), if value { "true" } else { "false" }.into());
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        let mut buf = itoa::Buffer::new();
        let s = buf.format(value);
        self.0.insert(field.name().into(), s.to_owned().into());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        let mut buf = itoa::Buffer::new();
        let s = buf.format(value);
        self.0.insert(field.name().into(), s.to_owned().into());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().into(), format!("{value:?}").into());
    }
}

impl Labels {
    fn from_record(record: &Record) -> Labels {
        let mut labels = Labels::default();
        record.record(&mut labels);
        labels
    }
}

impl AsRef<Map> for Labels {
    fn as_ref(&self) -> &Map {
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
        f: &mut dyn FnMut(Map) -> Option<Key>,
    ) -> Option<Key> {
        let mut ff = |labels: &Labels| f(labels.0.clone());
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

        let ext = span.extensions();
        f(ext.get::<Labels>()?)
    }
}

impl<S> Layer<S> for MetricsLayer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, cx: Context<'_, S>) {
        let span = cx.span(id).expect("span must already exist!");
        let mut labels = Labels::from_record(&Record::new(attrs.values()));

        if let Some(parent) = span.parent() {
            if let Some(parent_labels) = parent.extensions().get::<Labels>() {
                labels.extend_from_labels(parent_labels);
            }
        }

        span.extensions_mut().insert(labels);
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, cx: Context<'_, S>) {
        let span = cx.span(id).expect("span must already exist!");
        let labels = Labels::from_record(values);

        let ext = &mut span.extensions_mut();
        if let Some(existing) = ext.get_mut::<Labels>() {
            existing.extend_from_labels(&labels);
        } else {
            ext.insert(labels);
        }
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
