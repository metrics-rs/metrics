//! Records metrics in the Prometheus exposition format.
#![deny(missing_docs)]
use hdrhistogram::Histogram;
use metrics_core::{Builder, Drain, Key, Label, Observer};
use metrics_util::{parse_quantiles, Quantile};
use std::iter::FromIterator;
use std::{collections::HashMap, time::SystemTime};

/// Builder for [`PrometheusObserver`].
pub struct PrometheusBuilder {
    quantiles: Vec<Quantile>,
    buckets: Vec<u64>,
    buckets_by_name: Option<HashMap<String, Vec<u64>>>,
}

impl PrometheusBuilder {
    /// Creates a new [`PrometheusBuilder`] with default values.
    pub fn new() -> Self {
        let quantiles = parse_quantiles(&[0.0, 0.5, 0.9, 0.95, 0.99, 0.999, 1.0]);

        Self {
            quantiles,
            buckets: vec![],
            buckets_by_name: None,
        }
    }

    /// Sets the quantiles to use when rendering histograms.
    ///
    /// Quantiles represent a scale of 0 to 1, where percentiles represent a scale of 1 to 100, so
    /// a quantile of 0.99 is the 99th percentile, and a quantile of 0.99 is the 99.9th percentile.
    ///
    /// By default, the quantiles will be set to: 0.0, 0.5, 0.9, 0.95, 0.99, 0.999, and 1.0.
    pub fn set_quantiles(mut self, quantiles: &[f64]) -> Self {
        self.quantiles = parse_quantiles(quantiles);
        self
    }

    /// Sets the buckets to use when rendering summaries.
    ///
    /// Buckets values represent the higher bound of each buckets.
    pub fn set_buckets(mut self, values: &[u64]) -> Self {
        self.buckets = values.to_vec();
        self
    }

    /// Sets the buckets for a specific metric, overidding the default.
    ///
    /// Matches the metric name using `ends_with`.
    pub fn set_buckets_for_metric(mut self, name: &str, values: &[u64]) -> Self {
        let buckets = self.buckets_by_name.get_or_insert_with(|| HashMap::new());
        buckets.insert(name.to_owned(), values.to_vec());
        self
    }
}

impl Builder for PrometheusBuilder {
    type Output = PrometheusObserver;

    fn build(&self) -> Self::Output {
        PrometheusObserver {
            quantiles: self.quantiles.clone(),
            buckets: self.buckets.clone(),
            histos: HashMap::new(),
            output: get_prom_expo_header(),
            counters: HashMap::new(),
            gauges: HashMap::new(),
            buckets_by_name: self.buckets_by_name.clone(),
        }
    }
}

impl Default for PrometheusBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Records metrics in the Prometheus exposition format.
pub struct PrometheusObserver {
    pub(crate) quantiles: Vec<Quantile>,
    pub(crate) buckets: Vec<u64>,
    pub(crate) histos: HashMap<String, HashMap<Vec<String>, (u64, Histogram<u64>)>>,
    pub(crate) output: String,
    pub(crate) counters: HashMap<String, HashMap<Vec<String>, u64>>,
    pub(crate) gauges: HashMap<String, HashMap<Vec<String>, i64>>,
    pub(crate) buckets_by_name: Option<HashMap<String, Vec<u64>>>,
}

impl Observer for PrometheusObserver {
    fn observe_counter(&mut self, key: Key, value: u64) {
        let (name, labels) = key_to_parts(key);

        let entry = self
            .counters
            .entry(name)
            .or_insert_with(|| HashMap::new())
            .entry(labels)
            .or_insert_with(|| 0);

        *entry += value;
    }

    fn observe_gauge(&mut self, key: Key, value: i64) {
        let (name, labels) = key_to_parts(key);

        let entry = self
            .gauges
            .entry(name)
            .or_insert_with(|| HashMap::new())
            .entry(labels)
            .or_insert_with(|| 0);

        *entry = value;
    }

    fn observe_histogram(&mut self, key: Key, values: &[u64]) {
        let (name, labels) = key_to_parts(key);

        let entry = self
            .histos
            .entry(name)
            .or_insert_with(|| HashMap::new())
            .entry(labels)
            .or_insert_with(|| {
                let h = Histogram::<u64>::new(3).expect("failed to create histogram");
                (0, h)
            });

        let (sum, h) = entry;
        for value in values {
            h.record(*value).expect("failed to observe histogram value");
            *sum += *value;
        }
    }
}

impl Drain<String> for PrometheusObserver {
    fn drain(&mut self) -> String {
        let mut output: String = self.output.drain(..).collect();

        for (name, mut by_labels) in self.counters.drain() {
            output.push_str("\n# TYPE ");
            output.push_str(name.as_str());
            output.push_str(" counter\n");
            for (labels, value) in by_labels.drain() {
                let full_name = render_labeled_name(&name, &labels);
                output.push_str(full_name.as_str());
                output.push_str(" ");
                output.push_str(value.to_string().as_str());
                output.push_str("\n");
            }
        }

        for (name, mut by_labels) in self.gauges.drain() {
            output.push_str("\n# TYPE ");
            output.push_str(name.as_str());
            output.push_str(" gauge\n");
            for (labels, value) in by_labels.drain() {
                let full_name = render_labeled_name(&name, &labels);
                output.push_str(full_name.as_str());
                output.push_str(" ");
                output.push_str(value.to_string().as_str());
                output.push_str("\n");
            }
        }
        let mut sorted_overrides = self
            .buckets_by_name
            .as_ref()
            .map(|h| Vec::from_iter(h.iter()))
            .unwrap_or_else(|| vec![]);
        sorted_overrides.sort_by(|(a, _), (b, _)| b.len().cmp(&a.len()));

        for (name, mut by_labels) in self.histos.drain() {
            let buckets = sorted_overrides
                .iter()
                .find_map(|(k, buckets)| {
                    if name.ends_with(*k) {
                        Some(*buckets)
                    } else {
                        None
                    }
                })
                .unwrap_or(&self.buckets);
            let use_quantiles = buckets.is_empty();

            output.push_str("\n# TYPE ");
            output.push_str(name.as_str());
            output.push_str(" ");
            output.push_str(if use_quantiles {
                "summary"
            } else {
                "histogram"
            });
            output.push_str("\n");

            for (labels, sh) in by_labels.drain() {
                let (sum, hist) = sh;

                if use_quantiles {
                    for quantile in &self.quantiles {
                        let value = hist.value_at_quantile(quantile.value());
                        let mut labels = labels.clone();
                        labels.push(format!("quantile=\"{}\"", quantile.value()));
                        let full_name = render_labeled_name(&name, &labels);
                        output.push_str(full_name.as_str());
                        output.push_str(" ");
                        output.push_str(value.to_string().as_str());
                        output.push_str("\n");
                    }
                } else {
                    for bucket in buckets {
                        let value = hist.count_between(0, *bucket);
                        let mut labels = labels.clone();
                        labels.push(format!("le=\"{}\"", bucket));
                        let bucket_name = format!("{}_bucket", name);
                        let full_name = render_labeled_name(&bucket_name, &labels);
                        output.push_str(full_name.as_str());
                        output.push_str(" ");
                        output.push_str(value.to_string().as_str());
                        output.push_str("\n");
                    }
                    let mut labels = labels.clone();
                    labels.push("le=\"Inf+\"".to_owned());
                    let bucket_name = format!("{}_bucket", name);
                    let full_name = render_labeled_name(&bucket_name, &labels);
                    output.push_str(full_name.as_str());
                    output.push_str(" ");
                    output.push_str(hist.len().to_string().as_str());
                    output.push_str("\n");
                }
                let sum_name = format!("{}_sum", name);
                let full_sum_name = render_labeled_name(&sum_name, &labels);
                output.push_str(full_sum_name.as_str());
                output.push_str(" ");
                output.push_str(sum.to_string().as_str());
                output.push_str("\n");
                let count_name = format!("{}_count", name);
                let full_count_name = render_labeled_name(&count_name, &labels);
                output.push_str(full_count_name.as_str());
                output.push_str(" ");
                output.push_str(hist.len().to_string().as_str());
                output.push_str("\n");
            }
        }

        output
    }
}

fn key_to_parts(key: Key) -> (String, Vec<String>) {
    let (name, labels) = key.into_parts();
    let sanitize = |c| c == '.' || c == '=' || c == '{' || c == '}' || c == '+' || c == '-';
    let name = name.replace(sanitize, "_");
    let labels = labels
        .into_iter()
        .map(Label::into_parts)
        .map(|(k, v)| {
            format!(
                "{}=\"{}\"",
                k,
                v.replace("\\", "\\\\")
                    .replace("\"", "\\\"")
                    .replace("\n", "\\n")
            )
        })
        .collect();

    (name, labels)
}

fn render_labeled_name(name: &str, labels: &[String]) -> String {
    let mut output = name.to_string();
    if !labels.is_empty() {
        let joined = labels.join(",");
        output.push_str("{");
        output.push_str(&joined);
        output.push_str("}");
    }
    output
}

fn get_prom_expo_header() -> String {
    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    format!(
        "# metrics snapshot (ts={}) (prometheus exposition format)",
        ts
    )
}
