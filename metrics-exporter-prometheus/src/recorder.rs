use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::{PoisonError, RwLock};

use indexmap::IndexMap;
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Recorder, SharedString, Unit};
use metrics_util::registry::{Recency, Registry};
use quanta::Instant;

use crate::common::Snapshot;
use crate::distribution::{Distribution, DistributionBuilder};
use crate::formatting::{
    key_to_parts, sanitize_metric_name, write_help_line, write_metric_line, write_type_line,
};
use crate::registry::GenerationalAtomicStorage;

pub(crate) struct Inner {
    pub registry: Registry<Key, GenerationalAtomicStorage>,
    pub recency: Recency<Key>,
    pub distributions: RwLock<HashMap<String, IndexMap<Vec<String>, Distribution>>>,
    pub distribution_builder: DistributionBuilder,
    pub descriptions: RwLock<HashMap<String, SharedString>>,
    pub global_labels: IndexMap<String, String>,
}

impl Inner {
    fn get_recent_metrics(&self) -> Snapshot {
        let mut counters = HashMap::new();
        let counter_handles = self.registry.get_counter_handles();
        for (key, counter) in counter_handles {
            let gen = counter.get_generation();
            if !self.recency.should_store_counter(&key, gen, &self.registry) {
                continue;
            }

            let (name, labels) = key_to_parts(&key, Some(&self.global_labels));
            let value = counter.get_inner().load(Ordering::Acquire);
            let entry =
                counters.entry(name).or_insert_with(HashMap::new).entry(labels).or_insert(0);
            *entry = value;
        }

        let mut gauges = HashMap::new();
        let gauge_handles = self.registry.get_gauge_handles();
        for (key, gauge) in gauge_handles {
            let gen = gauge.get_generation();
            if !self.recency.should_store_gauge(&key, gen, &self.registry) {
                continue;
            }

            let (name, labels) = key_to_parts(&key, Some(&self.global_labels));
            let value = f64::from_bits(gauge.get_inner().load(Ordering::Acquire));
            let entry =
                gauges.entry(name).or_insert_with(HashMap::new).entry(labels).or_insert(0.0);
            *entry = value;
        }

        let histogram_handles = self.registry.get_histogram_handles();
        for (key, histogram) in histogram_handles {
            let gen = histogram.get_generation();
            if !self.recency.should_store_histogram(&key, gen, &self.registry) {
                // Since we store aggregated distributions directly, when we're told that a metric
                // is not recent enough and should be/was deleted from the registry, we also need to
                // delete it on our side as well.
                let (name, labels) = key_to_parts(&key, Some(&self.global_labels));
                let mut wg = self.distributions.write().unwrap_or_else(PoisonError::into_inner);
                let delete_by_name = if let Some(by_name) = wg.get_mut(&name) {
                    by_name.remove(&labels);
                    by_name.is_empty()
                } else {
                    false
                };

                // If there's no more variants in the per-metric-name distribution map, then delete
                // it entirely, otherwise we end up with weird empty output during render.
                if delete_by_name {
                    wg.remove(&name);
                }

                continue;
            }

            let (name, labels) = key_to_parts(&key, Some(&self.global_labels));

            let mut wg = self.distributions.write().unwrap_or_else(PoisonError::into_inner);
            let entry = wg
                .entry(name.clone())
                .or_insert_with(IndexMap::new)
                .entry(labels)
                .or_insert_with(|| self.distribution_builder.get_distribution(name.as_str()));

            histogram.get_inner().clear_with(|samples| entry.record_samples(samples));
        }

        let distributions =
            self.distributions.read().unwrap_or_else(PoisonError::into_inner).clone();

        Snapshot { counters, gauges, distributions }
    }

    fn render(&self) -> String {
        let Snapshot { mut counters, mut distributions, mut gauges } = self.get_recent_metrics();

        let mut output = String::new();
        let descriptions = self.descriptions.read().unwrap_or_else(PoisonError::into_inner);

        for (name, mut by_labels) in counters.drain() {
            if let Some(desc) = descriptions.get(name.as_str()) {
                write_help_line(&mut output, name.as_str(), desc);
            }

            write_type_line(&mut output, name.as_str(), "counter");
            for (labels, value) in by_labels.drain() {
                write_metric_line::<&str, u64>(&mut output, &name, None, &labels, None, value);
            }
            output.push('\n');
        }

        for (name, mut by_labels) in gauges.drain() {
            if let Some(desc) = descriptions.get(name.as_str()) {
                write_help_line(&mut output, name.as_str(), desc);
            }

            write_type_line(&mut output, name.as_str(), "gauge");
            for (labels, value) in by_labels.drain() {
                write_metric_line::<&str, f64>(&mut output, &name, None, &labels, None, value);
            }
            output.push('\n');
        }

        for (name, mut by_labels) in distributions.drain() {
            if let Some(desc) = descriptions.get(name.as_str()) {
                write_help_line(&mut output, name.as_str(), desc);
            }

            let distribution_type = self.distribution_builder.get_distribution_type(name.as_str());
            write_type_line(&mut output, name.as_str(), distribution_type);
            for (labels, distribution) in by_labels.drain(..) {
                let (sum, count) = match distribution {
                    Distribution::Summary(summary, quantiles, sum) => {
                        let snapshot = summary.snapshot(Instant::now());
                        for quantile in quantiles.iter() {
                            let value = snapshot.quantile(quantile.value()).unwrap_or(0.0);
                            write_metric_line(
                                &mut output,
                                &name,
                                None,
                                &labels,
                                Some(("quantile", quantile.value())),
                                value,
                            );
                        }

                        (sum, summary.count() as u64)
                    }
                    Distribution::Histogram(histogram) => {
                        for (le, count) in histogram.buckets() {
                            write_metric_line(
                                &mut output,
                                &name,
                                Some("bucket"),
                                &labels,
                                Some(("le", le)),
                                count,
                            );
                        }
                        write_metric_line(
                            &mut output,
                            &name,
                            Some("bucket"),
                            &labels,
                            Some(("le", "+Inf")),
                            histogram.count(),
                        );

                        (histogram.sum(), histogram.count())
                    }
                };

                write_metric_line::<&str, f64>(&mut output, &name, Some("sum"), &labels, None, sum);
                write_metric_line::<&str, u64>(
                    &mut output,
                    &name,
                    Some("count"),
                    &labels,
                    None,
                    count,
                );
            }

            output.push('\n');
        }

        output
    }
}

/// A Prometheus recorder.
///
/// This recorder should be composed with other recorders or installed globally via
/// [`metrics::set_boxed_recorder`].
///
/// Most users will not need to interact directly with the recorder, and can simply deal with the
/// builder methods on [`PrometheusBuilder`](crate::PrometheusBuilder) for building and installing
/// the recorder/exporter.
pub struct PrometheusRecorder {
    inner: Arc<Inner>,
}

impl PrometheusRecorder {
    /// Gets a [`PrometheusHandle`] to this recorder.
    pub fn handle(&self) -> PrometheusHandle {
        PrometheusHandle { inner: self.inner.clone() }
    }

    fn add_description_if_missing(&self, key_name: &KeyName, description: SharedString) {
        let sanitized = sanitize_metric_name(key_name.as_str());
        let mut descriptions =
            self.inner.descriptions.write().unwrap_or_else(PoisonError::into_inner);
        descriptions.entry(sanitized).or_insert(description);
    }
}

impl From<Inner> for PrometheusRecorder {
    fn from(inner: Inner) -> Self {
        PrometheusRecorder { inner: Arc::new(inner) }
    }
}

impl Recorder for PrometheusRecorder {
    fn describe_counter(&self, key_name: KeyName, _unit: Option<Unit>, description: SharedString) {
        self.add_description_if_missing(&key_name, description);
    }

    fn describe_gauge(&self, key_name: KeyName, _unit: Option<Unit>, description: SharedString) {
        self.add_description_if_missing(&key_name, description);
    }

    fn describe_histogram(
        &self,
        key_name: KeyName,
        _unit: Option<Unit>,
        description: SharedString,
    ) {
        self.add_description_if_missing(&key_name, description);
    }

    fn register_counter(&self, key: &Key) -> Counter {
        self.inner.registry.get_or_create_counter(key, |c| c.clone().into())
    }

    fn register_gauge(&self, key: &Key) -> Gauge {
        self.inner.registry.get_or_create_gauge(key, |c| c.clone().into())
    }

    fn register_histogram(&self, key: &Key) -> Histogram {
        self.inner.registry.get_or_create_histogram(key, |c| c.clone().into())
    }
}

/// Handle for accessing metrics stored via [`PrometheusRecorder`].
///
/// In certain scenarios, it may be necessary to directly handle requests that would otherwise be
/// handled directly by the HTTP listener, or push gateway background task.  [`PrometheusHandle`]
/// allows rendering a snapshot of the current metrics stored by an installed [`PrometheusRecorder`]
/// as a payload conforming to the Prometheus exposition format.
#[derive(Clone)]
pub struct PrometheusHandle {
    inner: Arc<Inner>,
}

impl PrometheusHandle {
    /// Takes a snapshot of the metrics held by the recorder and generates a payload conforming to
    /// the Prometheus exposition format.
    pub fn render(&self) -> String {
        self.inner.render()
    }
}
