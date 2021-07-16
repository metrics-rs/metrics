//! The code that integrates with the `tracing` crate.

use lockfree_object_pool::{LinearObjectPool, LinearOwnedReusable};
use metrics::Label;
use once_cell::sync::OnceCell;
use smallvec::SmallVec;
use std::sync::Arc;
use std::{any::TypeId, marker::PhantomData};
use tracing_core::span::{Attributes, Id, Record};
use tracing_core::{field::Visit, Dispatch, Field, Subscriber};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

fn get_pool() -> &'static Arc<LinearObjectPool<SmallVec<[Label; 4]>>> {
    static POOL: OnceCell<Arc<LinearObjectPool<SmallVec<[Label; 4]>>>> = OnceCell::new();
    POOL.get_or_init(|| Arc::new(LinearObjectPool::new(|| SmallVec::new(), |vec| vec.clear())))
}
/// Per-span extension for collecting labels from fields.
///
/// Hidden from documentation as there is no need for end users to ever touch this type, but it must
/// be public in order to be pulled in by external benchmark code.
#[doc(hidden)]
pub struct Labels(pub LinearOwnedReusable<SmallVec<[Label; 4]>>);

impl Visit for Labels {
    fn record_str(&mut self, field: &Field, value: &str) {
        let label = Label::new(field.name(), value.to_owned());
        self.0.push(label);
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        let label = Label::from_static_parts(field.name(), if value { "true" } else { "false" });
        self.0.push(label);
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        // Maximum length is 20 characters but 32 is a nice power-of-two number.
        let mut s = String::with_capacity(32);
        itoa::fmt(&mut s, value).expect("failed to format/write i64");
        let label = Label::new(field.name(), s);
        self.0.push(label);
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        // Maximum length is 20 characters but 32 is a nice power-of-two number.
        let mut s = String::with_capacity(32);
        itoa::fmt(&mut s, value).expect("failed to format/write u64");
        let label = Label::new(field.name(), s);
        self.0.push(label);
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let value_string = format!("{:?}", value);
        let label = Label::new(field.name(), value_string);
        self.0.push(label);
    }
}

impl Labels {
    fn from_attributes(attrs: &Attributes<'_>) -> Self {
        let labels = get_pool().pull_owned();
        let mut labels = Self(labels);
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
    with_labels: fn(&Dispatch, &Id, f: &mut dyn FnMut(&Labels)),
}

impl WithContext {
    pub fn with_labels<'a>(&self, dispatch: &'a Dispatch, id: &Id, f: &mut dyn FnMut(&[Label])) {
        let mut ff = |labels: &Labels| f(labels.as_ref());
        (self.with_labels)(dispatch, id, &mut ff)
    }
}

/// [`MetricsLayer`] is a [`tracing_subscriber::Layer`] that captures the span
/// fields and allows them to be later on used as metrics labels.
pub struct MetricsLayer<S> {
    ctx: WithContext,
    _subscriber: PhantomData<fn(S)>,
    _priv: (),
}

impl<S> MetricsLayer<S>
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    /// Create a new `MetricsLayer`.
    pub fn new() -> Self {
        let ctx = WithContext {
            with_labels: Self::with_labels,
        };

        Self {
            ctx,
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

        let parents = span.parents();
        for span in std::iter::once(span).chain(parents) {
            let extensions = span.extensions();
            if let Some(value) = extensions.get::<Labels>() {
                f(value);
            }
        }
    }
}

impl<S> Layer<S> for MetricsLayer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn new_span(&self, attrs: &Attributes<'_>, id: &Id, cx: Context<'_, S>) {
        let span = cx.span(id).expect("span must already exist!");
        let labels = Labels::from_attributes(attrs);
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

/// An extention to the `tracing::Span`, enabling the access to labels.
pub trait SpanExt {
    /// Run the provided function with a read-only access to labels.
    fn with_labels<F>(&self, f: F)
    where
        F: FnMut(&[Label]);
}

impl SpanExt for tracing::Span {
    fn with_labels<F>(&self, mut f: F)
    where
        F: FnMut(&[Label]),
    {
        self.with_subscriber(|(id, subscriber)| {
            if let Some(ctx) = subscriber.downcast_ref::<WithContext>() {
                ctx.with_labels(subscriber, id, &mut f)
            }
        });
    }
}
