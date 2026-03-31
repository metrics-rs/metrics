//! Format-agnostic structured representation of a Prometheus metrics snapshot.
//!
//! The types in this module mirror the Prometheus data model (metric families,
//! individual metrics with labels, and typed values) without being tied to a
//! specific wire format. They serve as an intermediate representation that can
//! be converted into protobuf, text exposition, or consumed directly by
//! application code via [`PrometheusHandle::render_snapshot_and_descriptions`].
//!
//! [`PrometheusHandle::render_snapshot_and_descriptions`]: crate::PrometheusHandle::render_snapshot_and_descriptions

use crate::common::Snapshot;
use crate::formatting::sanitize_metric_name;
use crate::recorder::DescriptionReadHandle;
use crate::LabelSet;

/// A rendered snapshot of all metric families.
pub type RenderedMetrics = Vec<MetricFamily>;

/// A single label key-value pair attached to a metric sample.
#[derive(Debug)]
pub struct LabelPair {
    /// The label name (e.g. `"method"`).
    pub label: String,
    /// The label value (e.g. `"GET"`).
    pub value: String,
}

/// A named group of metrics that share the same metric name, help text, and
/// value type — corresponding to a single Prometheus `MetricFamily`.
#[derive(Debug)]
pub struct MetricFamily {
    /// The sanitized metric name, including any applicable suffix (e.g.
    /// `"http_requests_total"`).
    pub name: String,
    /// The `HELP` description, if one was registered.
    pub help: Option<String>,
    /// The individual time-series samples within this family, each
    /// distinguished by its label set.
    pub metrics: Vec<Metric>,
}

/// A single time-series sample: a set of labels and a typed value.
#[derive(Debug)]
pub struct Metric {
    /// Labels that identify this particular time series.
    pub labels: Vec<LabelPair>,
    /// The typed metric value.
    pub value: MetricValue,
}

/// The typed payload of a metric sample.
#[derive(Debug)]
pub enum MetricValue {
    /// A monotonically increasing counter, stored as a raw `u64` count.
    Counter(u64),
    /// A gauge that can go up and down.
    Gauge(f64),
    /// A client-side computed summary with pre-calculated quantiles.
    Summary(Summary),
    /// A classic Prometheus histogram with fixed upper-bound buckets.
    ClassicHistogram(ClassicHistogram),
    /// A Prometheus native (exponential) histogram with sparse bucket spans.
    NativeHistogram(NativeHistogram),
}

/// A single quantile measurement within a [`Summary`].
#[derive(Debug)]
pub struct Quantile {
    /// The quantile rank in `[0.0, 1.0]` (e.g. `0.99` for the 99th percentile).
    pub quantile: f64,
    /// The observed value at this quantile.
    pub value: f64,
}

/// A Prometheus summary: pre-computed quantiles plus total count and sum.
#[derive(Debug)]
pub struct Summary {
    /// Total number of observations.
    pub sample_count: u64,
    /// Sum of all observed values.
    pub sample_sum: f64,
    /// Pre-computed quantile values.
    pub quantiles: Vec<Quantile>,
}

/// A single bucket in a classic histogram.
#[derive(Debug)]
pub struct Bucket {
    /// Cumulative count of observations that fall at or below [`upper_bound`](Self::upper_bound).
    pub cumulative_count: u64,
    /// The inclusive upper bound of this bucket. The final bucket uses
    /// [`f64::INFINITY`] to represent the `+Inf` boundary.
    pub upper_bound: f64,
}

/// A classic (fixed-bucket) Prometheus histogram.
#[derive(Debug)]
pub struct ClassicHistogram {
    /// Total number of observations.
    pub sample_count: u64,
    /// Sum of all observed values.
    pub sample_sum: f64,
    /// The histogram buckets, including the `+Inf` sentinel.
    pub buckets: Vec<Bucket>,
}

/// A contiguous run of populated buckets in a native histogram, encoded as an
/// offset from the previous span's end and a length.
///
/// See the [Prometheus native histogram design doc][nhd] for details.
///
/// [nhd]: https://docs.google.com/document/d/1cLNv3aufPZb3fNfaJgdCRBnkiEEMBufqCMm1Yj7LSEI
#[derive(Debug)]
pub struct BucketSpan {
    /// Signed offset from the expected next bucket index to the start of this
    /// span.
    pub offset: i32,
    /// Number of consecutive populated buckets in this span.
    pub length: u32,
}

/// A Prometheus native (exponential) histogram.
///
/// Native histograms use a logarithmic bucket scheme defined by a `schema`
/// exponent, with sparse encoding via [`BucketSpan`]s and delta-encoded counts.
#[derive(Debug)]
pub struct NativeHistogram {
    /// Total number of observations.
    pub sample_count: u64,
    /// Sum of all observed values.
    pub sample_sum: f64,
    /// Observations with an absolute value at or below this threshold are
    /// counted in [`zero_count`](Self::zero_count) instead of a regular bucket.
    pub zero_threshold: f64,
    /// The exponential schema controlling bucket boundaries (e.g. `3` for
    /// `2^(2^-3)` growth factor). Lower values produce wider buckets.
    pub schema: i32,
    /// Count of observations within the zero bucket.
    pub zero_count: u64,

    /// Spans describing contiguous runs of positive-value buckets.
    pub positive_spans: Vec<BucketSpan>,
    /// Delta-encoded counts for positive-value buckets (one per bucket across
    /// all positive spans).
    pub positive_deltas: Vec<i64>,

    /// Spans describing contiguous runs of negative-value buckets.
    pub negative_spans: Vec<BucketSpan>,
    /// Delta-encoded counts for negative-value buckets.
    pub negative_deltas: Vec<i64>,
}

pub(crate) fn render_snapshot_and_descriptions(
    snapshot: Snapshot,
    descriptions_rd: &DescriptionReadHandle,
    counter_suffix: Option<&'static str>,
) -> RenderedMetrics {
    let counters = snapshot
        .counters
        .into_iter()
        .map(|(name, by_labels)| render_counter(&name, by_labels, descriptions_rd, counter_suffix));

    let gauges = snapshot
        .gauges
        .into_iter()
        .map(|(name, by_labels)| render_gauge(&name, by_labels, descriptions_rd));

    let distributions = snapshot
        .distributions
        .into_iter()
        .map(|(name, by_labels)| render_distribution(&name, by_labels, descriptions_rd));

    counters.chain(gauges).chain(distributions).collect()
}

pub(crate) fn render_counter(
    name: &str,
    by_labels: std::collections::HashMap<LabelSet, u64>,
    descriptions_rd: &DescriptionReadHandle,
    counter_suffix: Option<&'static str>,
) -> MetricFamily {
    render_metric(name, by_labels, descriptions_rd, counter_suffix, MetricValue::Counter)
}

pub(crate) fn render_gauge(
    name: &str,
    by_labels: std::collections::HashMap<LabelSet, f64>,
    descriptions_rd: &DescriptionReadHandle,
) -> MetricFamily {
    render_metric(name, by_labels, descriptions_rd, None, MetricValue::Gauge)
}

pub(crate) fn render_distribution(
    name: &str,
    by_labels: indexmap::IndexMap<LabelSet, crate::Distribution>,
    descriptions_rd: &DescriptionReadHandle,
) -> MetricFamily {
    render_metric(name, by_labels, descriptions_rd, None, render_distribution_value)
}

fn get_help(name: &str, descriptions_rd: &DescriptionReadHandle) -> Option<String> {
    descriptions_rd
        .get_one(name)
        .as_deref()
        .map(|(desc, _)| desc.clone().into_owned())
        .filter(|desc| !desc.is_empty())
}

fn get_labels(labels: LabelSet) -> Vec<LabelPair> {
    labels.labels.into_iter().map(|(label, value)| LabelPair { label, value }).collect()
}

fn render_metric<T>(
    name: &str,
    by_labels: impl IntoIterator<Item = (LabelSet, T)>,
    descriptions_rd: &DescriptionReadHandle,
    counter_suffix: Option<&'static str>,
    mut render_value: impl FnMut(T) -> MetricValue,
) -> MetricFamily {
    MetricFamily {
        name: add_suffix_to_name(sanitize_metric_name(name), counter_suffix),
        help: get_help(name, descriptions_rd),
        metrics: by_labels
            .into_iter()
            .map(|(labels, value)| Metric {
                labels: get_labels(labels),
                value: render_value(value),
            })
            .collect(),
    }
}

fn add_suffix_to_name(name: String, suffix: Option<&'static str>) -> String {
    match suffix {
        Some(suffix) if !name.ends_with(suffix) => format!("{name}_{suffix}"),
        _ => name,
    }
}

#[expect(clippy::needless_pass_by_value, reason = "matches FnMut signature")]
fn render_distribution_value(distribution: crate::Distribution) -> MetricValue {
    match &distribution {
        crate::Distribution::Summary(summary, quantiles, sum) => {
            render_summary(summary, quantiles, *sum)
        }
        crate::Distribution::Histogram(histogram) => render_classic_histogram(histogram),
        crate::Distribution::NativeHistogram(histogram) => render_native_histogram(histogram),
    }
}

fn render_summary(
    summary: &crate::distribution::RollingSummary,
    quantiles: &[metrics_util::Quantile],
    sum: f64,
) -> MetricValue {
    let snapshot = summary.snapshot(quanta::Instant::now());
    MetricValue::Summary(Summary {
        sample_count: summary.count() as u64,
        sample_sum: sum,
        quantiles: quantiles
            .iter()
            .map(|q| Quantile {
                quantile: q.value(),
                value: snapshot.quantile(q.value()).unwrap_or(0.0),
            })
            .collect(),
    })
}

fn render_classic_histogram(histogram: &metrics_util::storage::Histogram) -> MetricValue {
    let buckets = histogram
        .buckets()
        .into_iter()
        // Add +Inf bucket
        .chain(std::iter::once((f64::INFINITY, histogram.count())))
        .map(|(upper_bound, cumulative_count)| Bucket { cumulative_count, upper_bound })
        .collect();
    MetricValue::ClassicHistogram(ClassicHistogram {
        sample_count: histogram.count(),
        sample_sum: histogram.sum(),
        buckets,
    })
}

fn render_native_histogram(histogram: &crate::NativeHistogram) -> MetricValue {
    let (positive_spans, positive_deltas) = make_buckets(histogram.positive_buckets());
    let (negative_spans, negative_deltas) = make_buckets(histogram.negative_buckets());
    let mut native_histogram = NativeHistogram {
        sample_count: histogram.count(),
        sample_sum: histogram.sum(),
        zero_threshold: histogram.config().zero_threshold(),
        schema: histogram.schema(),
        zero_count: histogram.zero_count(),
        positive_spans,
        positive_deltas,
        negative_spans,
        negative_deltas,
    };

    // Add a no-op span if histogram is empty (matches Go implementation)
    if native_histogram.zero_threshold == 0.0
        && native_histogram.zero_count == 0
        && native_histogram.positive_spans.is_empty()
        && native_histogram.negative_spans.is_empty()
    {
        native_histogram.positive_spans.push(BucketSpan { offset: 0, length: 0 });
    }

    MetricValue::NativeHistogram(native_histogram)
}

fn make_buckets(buckets: std::collections::BTreeMap<i32, u64>) -> (Vec<BucketSpan>, Vec<i64>) {
    if buckets.is_empty() {
        return (vec![], vec![]);
    }

    let mut spans = Vec::new();
    let mut deltas = Vec::new();
    let mut prev_count = 0i64;
    let mut next_i = 0i32;
    let mut first = true;

    for (i, count) in buckets {
        #[expect(clippy::cast_possible_wrap)]
        let count = count as i64;

        // Multiple spans with only small gaps in between are probably
        // encoded more efficiently as one larger span with a few empty buckets.
        // Following Go: gaps of one or two buckets should not create a new span.
        let i_delta = i - next_i;

        if first || i_delta > 2 {
            first = false;
            // Create a new span - either first bucket or gap > 2
            spans.push(BucketSpan { offset: i_delta, length: 0 });
        } else {
            // Small gap (or no gap) - insert empty buckets as needed
            for _ in 0..i_delta {
                if let Some(last_span) = spans.last_mut() {
                    last_span.length += 1;
                }
                deltas.push(-prev_count);
                prev_count = 0;
            }
        }

        // Add the current bucket
        if let Some(last_span) = spans.last_mut() {
            last_span.length += 1;
        }
        deltas.push(count - prev_count);
        prev_count = count;
        next_i = i + 1;
    }

    (spans, deltas)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_suffix_to_name() {
        assert_eq!(add_suffix_to_name("requests".to_owned(), Some("total")), "requests_total");
        assert_eq!(
            add_suffix_to_name("requests_total".to_owned(), Some("total")),
            "requests_total"
        );
        assert_eq!(add_suffix_to_name("requests".to_owned(), None), "requests");
    }
}
