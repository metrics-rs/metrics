use std::collections::HashMap;
use std::io;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::{PoisonError, RwLock};

use indexmap::IndexMap;
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};
use metrics_util::registry::{Recency, Registry};
use quanta::Instant;

use crate::common::{LabelSet, Snapshot};
use crate::distribution::{Distribution, DistributionBuilder};
use crate::formatting::{
    sanitize_metric_name, write_help_line, write_metric_line, write_type_line,
};
use crate::registry::GenerationalAtomicStorage;

#[derive(Debug)]
pub(crate) struct Inner {
    pub registry: Registry<Key, GenerationalAtomicStorage>,
    pub recency: Recency<Key>,
    pub distributions: RwLock<HashMap<String, IndexMap<LabelSet, Distribution>>>,
    pub distribution_builder: DistributionBuilder,
    pub descriptions: RwLock<HashMap<String, (SharedString, Option<Unit>)>>,
    pub global_labels: IndexMap<String, String>,
    pub enable_unit_suffix: bool,
    pub counter_suffix: Option<&'static str>,
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

            let name = sanitize_metric_name(key.name());
            let labels = LabelSet::from_key_and_global(&key, &self.global_labels);
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

            let name = sanitize_metric_name(key.name());
            let labels = LabelSet::from_key_and_global(&key, &self.global_labels);
            let value = f64::from_bits(gauge.get_inner().load(Ordering::Acquire));
            let entry =
                gauges.entry(name).or_insert_with(HashMap::new).entry(labels).or_insert(0.0);
            *entry = value;
        }

        // Update distributions
        self.drain_histograms_to_distributions();
        // Remove expired histograms
        let histogram_handles = self.registry.get_histogram_handles();
        for (key, histogram) in histogram_handles {
            let gen = histogram.get_generation();
            if !self.recency.should_store_histogram(&key, gen, &self.registry) {
                // Since we store aggregated distributions directly, when we're told that a metric
                // is not recent enough and should be/was deleted from the registry, we also need to
                // delete it on our side as well.
                let name = sanitize_metric_name(key.name());
                let labels = LabelSet::from_key_and_global(&key, &self.global_labels);
                let mut wg = self.distributions.write().unwrap_or_else(PoisonError::into_inner);
                let delete_by_name = if let Some(by_name) = wg.get_mut(&name) {
                    by_name.swap_remove(&labels);
                    by_name.is_empty()
                } else {
                    false
                };

                // If there's no more variants in the per-metric-name distribution map, then delete
                // it entirely, otherwise we end up with weird empty output during render.
                if delete_by_name {
                    wg.remove(&name);
                }
            }
        }

        let distributions =
            self.distributions.read().unwrap_or_else(PoisonError::into_inner).clone();

        Snapshot { counters, gauges, distributions }
    }

    /// Drains histogram samples into distribution.
    fn drain_histograms_to_distributions(&self) {
        let histogram_handles = self.registry.get_histogram_handles();
        for (key, histogram) in histogram_handles {
            let name = sanitize_metric_name(key.name());
            let labels = LabelSet::from_key_and_global(&key, &self.global_labels);

            let mut wg = self.distributions.write().unwrap_or_else(PoisonError::into_inner);
            let entry = wg
                .entry(name.clone())
                .or_default()
                .entry(labels)
                .or_insert_with(|| self.distribution_builder.get_distribution(name.as_str()));

            histogram.get_inner().clear_with(|samples| entry.record_samples(samples));
        }
    }

    fn render_to_write(&self, output: &mut impl io::Write) -> io::Result<()> {
        let Snapshot { mut counters, mut distributions, mut gauges } = self.get_recent_metrics();

        let mut intermediate = String::new();
        let descriptions = self.descriptions.read().unwrap_or_else(PoisonError::into_inner);

        for (name, mut by_labels) in counters.drain() {
            let unit = descriptions.get(name.as_str()).and_then(|(desc, unit)| {
                let unit = unit.filter(|_| self.enable_unit_suffix);
                write_help_line(&mut intermediate, name.as_str(), unit, self.counter_suffix, desc);
                unit
            });

            write_type_line(&mut intermediate, name.as_str(), unit, self.counter_suffix, "counter");

            // A chunk is emitted here, just in case there are a large number of sets below.
            output.write_all(intermediate.as_bytes())?;
            intermediate.clear();

            for (labels, value) in by_labels.drain() {
                write_metric_line::<&str, u64>(
                    &mut intermediate,
                    &name,
                    self.counter_suffix,
                    &labels,
                    None,
                    value,
                    unit,
                );
                // Each set gets its own write invocation.
                output.write_all(intermediate.as_bytes())?;
                intermediate.clear();
            }
            output.write_all(b"\n")?;
        }

        for (name, mut by_labels) in gauges.drain() {
            let unit = descriptions.get(name.as_str()).and_then(|(desc, unit)| {
                let unit = unit.filter(|_| self.enable_unit_suffix);
                write_help_line(&mut intermediate, name.as_str(), unit, None, desc);
                unit
            });

            write_type_line(&mut intermediate, name.as_str(), unit, None, "gauge");

            // A chunk is emitted here, just in case there are a large number of sets below.
            output.write_all(intermediate.as_bytes())?;
            intermediate.clear();

            for (labels, value) in by_labels.drain() {
                write_metric_line::<&str, f64>(
                    &mut intermediate,
                    &name,
                    None,
                    &labels,
                    None,
                    value,
                    unit,
                );
                // Each set gets its own write invocation.
                output.write_all(intermediate.as_bytes())?;
                intermediate.clear();
            }
            output.write_all(b"\n")?;
        }

        for (name, mut by_labels) in distributions.drain() {
            let distribution_type = self.distribution_builder.get_distribution_type(name.as_str());

            // Skip native histograms in text format - they're only supported in protobuf format
            if distribution_type == "native_histogram" {
                continue;
            }

            let unit = descriptions.get(name.as_str()).and_then(|(desc, unit)| {
                let unit = unit.filter(|_| self.enable_unit_suffix);
                write_help_line(&mut intermediate, name.as_str(), unit, None, desc);
                unit
            });

            write_type_line(&mut intermediate, name.as_str(), unit, None, distribution_type);

            // A chunk is emitted here, just in case there are a large number of sets below.
            output.write_all(intermediate.as_bytes())?;
            intermediate.clear();

            for (labels, distribution) in by_labels.drain(..) {
                let (sum, count) = match distribution {
                    Distribution::Summary(summary, quantiles, sum) => {
                        let snapshot = summary.snapshot(Instant::now());
                        for quantile in quantiles.iter() {
                            let value = snapshot.quantile(quantile.value()).unwrap_or(0.0);
                            write_metric_line(
                                &mut intermediate,
                                &name,
                                None,
                                &labels,
                                Some(("quantile", quantile.value())),
                                value,
                                unit,
                            );
                        }

                        (sum, summary.count() as u64)
                    }
                    Distribution::Histogram(histogram) => {
                        for (le, count) in histogram.buckets() {
                            write_metric_line(
                                &mut intermediate,
                                &name,
                                Some("bucket"),
                                &labels,
                                Some(("le", le)),
                                count,
                                unit,
                            );
                        }
                        write_metric_line(
                            &mut intermediate,
                            &name,
                            Some("bucket"),
                            &labels,
                            Some(("le", "+Inf")),
                            histogram.count(),
                            unit,
                        );

                        (histogram.sum(), histogram.count())
                    }
                    Distribution::NativeHistogram(_) => {
                        // Native histograms are not supported in text format
                        // This branch should not be reached due to the continue above
                        continue;
                    }
                };

                write_metric_line::<&str, f64>(
                    &mut intermediate,
                    &name,
                    Some("sum"),
                    &labels,
                    None,
                    sum,
                    unit,
                );
                write_metric_line::<&str, u64>(
                    &mut intermediate,
                    &name,
                    Some("count"),
                    &labels,
                    None,
                    count,
                    unit,
                );

                // Each set gets its own write invocation.
                output.write_all(intermediate.as_bytes())?;
                intermediate.clear();
            }

            output.write_all(b"\n")?;
        }

        Ok(())
    }

    fn run_upkeep(&self) {
        self.drain_histograms_to_distributions();
    }
}

/// A Prometheus recorder.
///
/// Most users will not need to interact directly with the recorder, and can simply deal with the
/// builder methods on [`PrometheusBuilder`](crate::PrometheusBuilder) for building and installing
/// the recorder/exporter.
#[derive(Debug)]
pub struct PrometheusRecorder {
    inner: Arc<Inner>,
}

impl PrometheusRecorder {
    /// Gets a [`PrometheusHandle`] to this recorder.
    pub fn handle(&self) -> PrometheusHandle {
        PrometheusHandle { inner: self.inner.clone() }
    }

    fn add_description_if_missing(
        &self,
        key_name: &KeyName,
        description: SharedString,
        unit: Option<Unit>,
    ) {
        let sanitized = sanitize_metric_name(key_name.as_str());
        let mut descriptions =
            self.inner.descriptions.write().unwrap_or_else(PoisonError::into_inner);
        descriptions.entry(sanitized).or_insert((description, unit));
    }
}

impl From<Inner> for PrometheusRecorder {
    fn from(inner: Inner) -> Self {
        PrometheusRecorder { inner: Arc::new(inner) }
    }
}

impl Recorder for PrometheusRecorder {
    fn describe_counter(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        self.add_description_if_missing(&key_name, description, unit);
    }

    fn describe_gauge(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        self.add_description_if_missing(&key_name, description, unit);
    }

    fn describe_histogram(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        self.add_description_if_missing(&key_name, description, unit);
    }

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        self.inner.registry.get_or_create_counter(key, |c| c.clone().into())
    }

    fn register_gauge(&self, key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        self.inner.registry.get_or_create_gauge(key, |c| c.clone().into())
    }

    fn register_histogram(&self, key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        self.inner.registry.get_or_create_histogram(key, |c| c.clone().into())
    }
}

/// Handle for accessing metrics stored via [`PrometheusRecorder`].
///
/// In certain scenarios, it may be necessary to directly handle requests that would otherwise be
/// handled directly by the HTTP listener, or push gateway background task.  [`PrometheusHandle`]
/// allows rendering a snapshot of the current metrics stored by an installed [`PrometheusRecorder`]
/// as a payload conforming to the Prometheus exposition format.
#[derive(Clone, Debug)]
pub struct PrometheusHandle {
    inner: Arc<Inner>,
}

impl PrometheusHandle {
    /// Takes a snapshot of the metrics held by the recorder and generates a payload conforming to
    /// the Prometheus exposition format.
    #[allow(clippy::missing_panics_doc)]
    pub fn render(&self) -> String {
        let mut buf = Vec::new();
        // UNWRAP: writing to a Vec<u8> does not fail.
        self.inner.render_to_write(&mut buf).unwrap();
        // UNWRAP: Prometheus exposition format is always UTF-8.
        String::from_utf8(buf).unwrap()
    }

    /// Takes a snapshot of the metrics held by the recorder and generates a payload conforming to
    /// the Prometheus exposition format, incrementally. Use this function to emit metrics as a
    /// stream without buffering the entire metrics export.
    ///
    /// # Errors
    ///
    /// Writing to the provided output fails.
    pub fn render_to_write(&self, output: &mut impl io::Write) -> io::Result<()> {
        self.inner.render_to_write(output)
    }

    /// Takes a snapshot of the metrics held by the recorder and generates a payload conforming to
    /// the Prometheus protobuf format.
    #[cfg(feature = "protobuf")]
    pub fn render_protobuf(&self) -> Vec<u8> {
        let snapshot = self.inner.get_recent_metrics();
        let descriptions = self.inner.descriptions.read().unwrap_or_else(PoisonError::into_inner);

        crate::protobuf::render_protobuf(snapshot, &descriptions, self.inner.counter_suffix)
    }

    /// Performs upkeeping operations to ensure metrics held by recorder are up-to-date and do not
    /// grow unboundedly.
    pub fn run_upkeep(&self) {
        self.inner.run_upkeep();
    }
}
