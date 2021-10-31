use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use metrics_util::recency::{GenerationalPrimitives, Recency};
use metrics_util::Registry;
use parking_lot::RwLock;

use metrics::{Counter, Gauge, Histogram, Key, Recorder, Unit};

use crate::common::{sanitize_key_name, Snapshot};
use crate::distribution::{Distribution, DistributionBuilder};

pub(crate) struct Inner {
    pub registry: Registry<GenerationalPrimitives>,
    pub recency: Recency,
    pub distributions: RwLock<HashMap<String, HashMap<Vec<String>, Distribution>>>,
    pub distribution_builder: DistributionBuilder,
    pub descriptions: RwLock<HashMap<String, &'static str>>,
    pub global_labels: HashMap<String, String>,
}

impl Inner {
    pub(crate) fn registry(&self) -> &Registry<GenerationalPrimitives> {
        &self.registry
    }

    fn get_recent_metrics(&self) -> Snapshot {
        let mut counters = HashMap::new();
        let counter_handles = self.registry.get_counter_handles();
        for (key, counter) in counter_handles {
            let gen = counter.get_generation();
            if !self
                .recency
                .should_store_counter(&key, gen, self.registry())
            {
                continue;
            }

            let (name, labels) = key_to_parts(&key, &self.global_labels);
            let value = counter.get_inner().load(Ordering::Acquire);
            let entry = counters
                .entry(name)
                .or_insert_with(HashMap::new)
                .entry(labels)
                .or_insert(0);
            *entry = value;
        }

        let mut gauges = HashMap::new();
        let gauge_handles = self.registry.get_gauge_handles();
        for (key, gauge) in gauge_handles {
            let gen = gauge.get_generation();
            if !self.recency.should_store_gauge(&key, gen, self.registry()) {
                continue;
            }

            let (name, labels) = key_to_parts(&key, &self.global_labels);
            let value = f64::from_bits(gauge.get_inner().load(Ordering::Acquire));
            let entry = gauges
                .entry(name)
                .or_insert_with(HashMap::new)
                .entry(labels)
                .or_insert(0.0);
            *entry = value;
        }

        let histogram_handles = self.registry.get_histogram_handles();
        for (key, histogram) in histogram_handles {
            let gen = histogram.get_generation();
            if !self.recency.should_store_gauge(&key, gen, self.registry()) {
                continue;
            }

            let (name, labels) = key_to_parts(&key, &self.global_labels);

            let mut wg = self.distributions.write();
            let entry = wg
                .entry(name.clone())
                .or_insert_with(HashMap::new)
                .entry(labels)
                .or_insert_with(|| {
                    self.distribution_builder
                        .get_distribution(name.as_str())
                        .expect("failed to create distribution")
                });

            histogram
                .get_inner()
                .clear_with(|samples| entry.record_samples(samples));
        }

        let distributions = self.distributions.read().clone();

        Snapshot {
            counters,
            gauges,
            distributions,
        }
    }

    pub fn render(&self) -> String {
        let Snapshot {
            mut counters,
            mut distributions,
            mut gauges,
        } = self.get_recent_metrics();

        let mut output = String::new();
        let descriptions = self.descriptions.read();

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

            let distribution_type = self
                .distribution_builder
                .get_distribution_type(name.as_str());
            write_type_line(&mut output, name.as_str(), distribution_type);
            for (labels, distribution) in by_labels.drain() {
                let (sum, count) = match distribution {
                    Distribution::Summary(summary, quantiles, sum) => {
                        for quantile in quantiles.iter() {
                            let value = summary.quantile(quantile.value()).unwrap_or(0.0);
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
///
pub struct PrometheusRecorder {
    inner: Arc<Inner>,
}

impl PrometheusRecorder {
    /// Gets a [`PrometheusHandle`] to this recorder.
    pub fn handle(&self) -> PrometheusHandle {
        PrometheusHandle {
            inner: self.inner.clone(),
        }
    }

    fn add_description_if_missing(&self, key: &Key, description: Option<&'static str>) {
        if let Some(description) = description {
            let mut descriptions = self.inner.descriptions.write();
            if !descriptions.contains_key(key.name().to_string().as_str()) {
                descriptions.insert(key.name().to_string(), description);
            }
        }
    }
}

impl From<Inner> for PrometheusRecorder {
    fn from(inner: Inner) -> Self {
        PrometheusRecorder {
            inner: Arc::new(inner),
        }
    }
}

impl Recorder for PrometheusRecorder {
    fn describe_counter(&self, key: &Key, _unit: Option<Unit>, description: Option<&'static str>) {
        self.add_description_if_missing(key, description);
    }

    fn describe_gauge(&self, key: &Key, _unit: Option<Unit>, description: Option<&'static str>) {
        self.add_description_if_missing(key, description);
    }

    fn describe_histogram(
        &self,
        key: &Key,
        _unit: Option<Unit>,
        description: Option<&'static str>,
    ) {
        self.add_description_if_missing(key, description);
    }

    fn register_counter(&self, key: &Key) -> Counter {
        self.inner
            .registry
            .get_or_create_counter(key, |c| c.get_inner().clone().into())
    }

    fn register_gauge(&self, key: &Key) -> Gauge {
        self.inner
            .registry
            .get_or_create_gauge(key, |c| c.get_inner().clone().into())
    }

    fn register_histogram(&self, key: &Key) -> Histogram {
        self.inner
            .registry
            .get_or_create_histogram(key, |c| c.get_inner().clone().into())
    }
}

/// Handle to [`PrometheusRecorder`].
///
/// Useful for exposing a scrape endpoint on an existing HTTP/HTTPS server.
#[derive(Clone)]
pub struct PrometheusHandle {
    inner: Arc<Inner>,
}

impl PrometheusHandle {
    /// Returns the metrics in Prometheus accepted String format.
    pub fn render(&self) -> String {
        self.inner.render()
    }
}

fn key_to_parts(key: &Key, defaults: &HashMap<String, String>) -> (String, Vec<String>) {
    let name = sanitize_key_name(key.name());
    let mut values = defaults.clone();
    key.labels().into_iter().for_each(|label| {
        values.insert(label.key().into(), label.value().into());
    });
    let labels = values
        .iter()
        .map(|(k, v)| {
            format!(
                "{}=\"{}\"",
                k,
                v.replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
            )
        })
        .collect();

    (name, labels)
}

fn write_help_line(buffer: &mut String, name: &str, desc: &str) {
    buffer.push_str("# HELP ");
    buffer.push_str(name);
    buffer.push(' ');
    buffer.push_str(desc);
    buffer.push('\n');
}

fn write_type_line(buffer: &mut String, name: &str, metric_type: &str) {
    buffer.push_str("# TYPE ");
    buffer.push_str(name);
    buffer.push(' ');
    buffer.push_str(metric_type);
    buffer.push('\n');
}

fn write_metric_line<T, T2>(
    buffer: &mut String,
    name: &str,
    suffix: Option<&'static str>,
    labels: &[String],
    additional_label: Option<(&'static str, T)>,
    value: T2,
) where
    T: std::fmt::Display,
    T2: std::fmt::Display,
{
    buffer.push_str(name);
    if let Some(suffix) = suffix {
        buffer.push('_');
        buffer.push_str(suffix)
    }

    if !labels.is_empty() || additional_label.is_some() {
        buffer.push('{');

        let mut first = true;
        for label in labels {
            if first {
                first = false;
            } else {
                buffer.push(',');
            }
            buffer.push_str(label);
        }

        if let Some((name, value)) = additional_label {
            if !first {
                buffer.push(',');
            }
            buffer.push_str(name);
            buffer.push_str("=\"");
            buffer.push_str(value.to_string().as_str());
            buffer.push('"');
        }

        buffer.push('}');
    }

    buffer.push(' ');
    buffer.push_str(value.to_string().as_str());
    buffer.push('\n');
}
