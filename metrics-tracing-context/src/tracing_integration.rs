//! The code that integrates with the `tracing` crate.

use std::{any::TypeId, marker::PhantomData};
use tracing_core::span::{Attributes, Id, Record};
use tracing_core::{field::Visit, Dispatch, Field, Subscriber};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

struct Labels(Vec<(&'static str, String)>);

impl Visit for Labels {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.push((field.name(), value.to_owned()));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let value = format!("{:?}", value);
        self.0.push((field.name(), value));
    }
}

impl Labels {
    fn from_attributes(attrs: &Attributes<'_>) -> Self {
        let mut labels = Self(Vec::new()); // TODO: Vec::with_capacity?
        let record = Record::new(attrs.values());
        record.record(&mut labels);
        labels
    }
}

impl AsRef<Vec<(&'static str, String)>> for Labels {
    fn as_ref(&self) -> &Vec<(&'static str, String)> {
        &self.0
    }
}

pub struct WithContext {
    with_labels: fn(&Dispatch, &Id, f: &mut dyn FnMut(&Labels)),
}

impl WithContext {
    pub fn with_labels<'a>(&self, dispatch: &'a Dispatch, id: &Id, f: &mut dyn FnMut(&Vec<(&'static str, String)>)) {
        let mut ff = |labels: &Labels| f(labels.as_ref());
        (self.with_labels)(dispatch, id, &mut ff)
    }
}

pub struct MetricsLayer<S> {
    ctx: WithContext,
    _subscriber: PhantomData<fn(S)>,
    _priv: (),
}

impl<S> MetricsLayer<S>
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
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

        let extensions = span.extensions();
        if let Some(value) = extensions.get::<Labels>() {
            f(value);
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

pub trait SpanExt {
    fn with_labels<F>(&self, f: F)
    where
        F: FnMut(&Vec<(&'static str, String)>);
}

impl SpanExt for tracing::Span {
    fn with_labels<F>(&self, mut f: F)
    where
        F: FnMut(&Vec<(&'static str, String)>),
    {
        self.with_subscriber(|(id, subscriber)| {
            if let Some(ctx) = subscriber.downcast_ref::<WithContext>() {
                ctx.with_labels(subscriber, id, &mut f)
            }
        });
    }
}