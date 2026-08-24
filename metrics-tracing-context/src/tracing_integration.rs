//! The code that integrates with the `tracing` crate.

use crate::field_merge::FieldMergePolicy;
use indexmap::map::Entry;
use indexmap::IndexMap;
use lockfree_object_pool::{LinearObjectPool, LinearOwnedReusable};
use metrics::{Key, SharedString};
use once_cell::sync::OnceCell;
use std::cmp;
use std::collections::HashMap;
use std::sync::Arc;
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
    fn extend(&mut self, other: &Labels, f: impl Fn(&mut Map, &SharedString, &SharedString)) {
        let new_len = cmp::max(self.as_ref().len(), other.as_ref().len());
        let additional = new_len - self.as_ref().len();
        self.0.reserve(additional);
        for (k, v) in other.as_ref() {
            f(&mut self.0, k, v);
        }
    }

    fn extend_from_labels(&mut self, other: &Labels) {
        self.extend(other, |map, k, v| {
            map.entry(k.clone()).or_insert_with(|| v.clone());
        });
    }

    fn extend_from_labels_overwrite(&mut self, other: &Labels) {
        self.extend(other, |map, k, v| {
            map.insert(k.clone(), v.clone());
        });
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

/// [`MetricsLayer`] is a [`tracing_subscriber::Layer`] that captures the span
/// fields and allows them to be later on used as metrics labels.
///
/// Merge behavior for individual field names can be customized via
/// [`MetricsLayer::with_field_merge_policy`].
#[derive(Default)]
pub struct MetricsLayer {
    #[allow(clippy::type_complexity)]
    with_labels:
        Option<fn(&Dispatch, &Id, f: &mut dyn FnMut(&Labels) -> Option<Key>) -> Option<Key>>,
    field_merges: HashMap<String, FieldMergePolicy>,
}

impl MetricsLayer {
    /// Create a new [`MetricsLayer`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Configures the [`FieldMergePolicy`] applied when merging values for the given
    /// span field name.
    ///
    /// Field names without an explicitly configured policy are merged using
    /// [`FieldMergePolicy::Override`], which preserves this crate's historical behavior.
    pub fn with_field_merge_policy(
        mut self,
        field: impl AsRef<str>,
        policy: FieldMergePolicy,
    ) -> Self {
        self.field_merges.insert(field.as_ref().to_owned(), policy);
        self
    }

    pub(crate) fn with_labels(
        &self,
        dispatch: &Dispatch,
        id: &Id,
        f: &mut dyn FnMut(Map) -> Option<Key>,
    ) -> Option<Key> {
        let mut ff = |labels: &Labels| f(labels.0.clone());
        (self.with_labels?)(dispatch, id, &mut ff)
    }
}

impl<S> Layer<S> for MetricsLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_layer(&mut self, _: &mut S) {
        self.with_labels = Some(|dispatch, id, f| {
            let subscriber = dispatch.downcast_ref::<S>()?;
            let span = subscriber.span(id)?;

            let ext = span.extensions();
            f(ext.get::<Labels>()?)
        });
    }

    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, cx: Context<'_, S>) {
        let span = cx.span(id).expect("span must already exist!");
        let mut labels = Labels::from_record(&Record::new(attrs.values()));

        if let Some(parent) = span.parent() {
            if let Some(parent_labels) = parent.extensions().get::<Labels>() {
                if self.field_merges.is_empty() {
                    labels.extend_from_labels(parent_labels);
                } else {
                    // Merging the parent's values over the child's own values: for
                    // `Append`, the parent's value is the outer value and the value
                    // already held by the child is the inner one.
                    let field_merges = &self.field_merges;
                    labels.extend(parent_labels, |map, k, v| {
                        match field_merges.get(k.as_ref()) {
                            Some(FieldMergePolicy::Append(sep)) => match map.entry(k.clone()) {
                                Entry::Occupied(mut entry) => {
                                    let composed = format!("{v}{sep}{}", entry.get());
                                    entry.insert(composed.into());
                                }
                                Entry::Vacant(entry) => {
                                    entry.insert(v.clone());
                                }
                            },
                            // `FieldMergePolicy::Override`: the child's own value wins.
                            _ => {
                                map.entry(k.clone()).or_insert_with(|| v.clone());
                            }
                        }
                    });
                }
            }
        }

        span.extensions_mut().insert(labels);
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, cx: Context<'_, S>) {
        let span = cx.span(id).expect("span must already exist!");
        let labels = Labels::from_record(values);

        let ext = &mut span.extensions_mut();
        if let Some(existing) = ext.get_mut::<Labels>() {
            if self.field_merges.is_empty() {
                existing.extend_from_labels_overwrite(&labels);
            } else {
                // Merging the newly recorded values over the values held so far: for
                // `Append`, the previously held value is the outer value and the newly
                // recorded value is the inner one.
                let field_merges = &self.field_merges;
                existing.extend(&labels, |map, k, v| match field_merges.get(k.as_ref()) {
                    Some(FieldMergePolicy::Append(sep)) => match map.entry(k.clone()) {
                        Entry::Occupied(mut entry) => {
                            let composed = format!("{}{sep}{v}", entry.get());
                            entry.insert(composed.into());
                        }
                        Entry::Vacant(entry) => {
                            entry.insert(v.clone());
                        }
                    },
                    // `FieldMergePolicy::Override`: the newly recorded value wins.
                    _ => {
                        map.insert(k.clone(), v.clone());
                    }
                });
            }
        } else {
            ext.insert(labels);
        }
    }
}
