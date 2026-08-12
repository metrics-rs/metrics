//! Format-agnostic structured representation of a Prometheus metrics snapshot.
//!
//! The types in this module mirror the Prometheus data model (metric families,
//! individual metrics with labels, and typed values) without being tied to a
//! specific wire format. They serve as an intermediate representation that can
//! be converted into protobuf, text exposition, or consumed directly by
//! application code via [`PrometheusHandle::render_snapshot_and_descriptions`].
//!
//! [`PrometheusHandle::render_snapshot_and_descriptions`]: crate::PrometheusHandle::render_snapshot_and_descriptions

use std::collections::hash_map::IntoIter as HashMapIntoIter;
use std::collections::HashMap;

use indexmap::IndexMap;

use crate::common::Snapshot;
use crate::formatting::sanitize_metric_name;
use crate::recorder::DescriptionReadHandle;
use crate::LabelSet;

/// An iterator over [`MetricFamily`] values produced from a metrics snapshot.
///
/// Created by [`render_snapshot_and_descriptions`]. Yields counter families
/// first, then gauge families, then distribution families. Implements
/// [`ExactSizeIterator`] so callers can pre-allocate or make layout decisions
/// before consuming.
#[derive(Debug)]
pub struct RenderedMetrics {
    counters: HashMapIntoIter<String, HashMap<LabelSet, u64>>,
    gauges: HashMapIntoIter<String, HashMap<LabelSet, f64>>,
    distributions: HashMapIntoIter<String, IndexMap<LabelSet, crate::Distribution>>,
    descriptions_rd: DescriptionReadHandle,
    counter_suffix: Option<&'static str>,
}

impl Iterator for RenderedMetrics {
    type Item = MetricFamily;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((name, by_labels)) = self.counters.next() {
            return Some(render_counter(
                &name,
                by_labels,
                &self.descriptions_rd,
                self.counter_suffix,
            ));
        }
        if let Some((name, by_labels)) = self.gauges.next() {
            return Some(render_gauge(&name, by_labels, &self.descriptions_rd));
        }
        if let Some((name, by_labels)) = self.distributions.next() {
            return Some(render_distribution(&name, by_labels, &self.descriptions_rd));
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.counters.len() + self.gauges.len() + self.distributions.len();
        (len, Some(len))
    }
}

impl ExactSizeIterator for RenderedMetrics {}

/// A single label key-value pair attached to a metric sample.
#[derive(Debug)]
#[non_exhaustive]
pub struct LabelPair {
    /// The label name (e.g. `"method"`).
    pub label: String,
    /// The label value (e.g. `"GET"`).
    pub value: String,
}

/// The Prometheus metric type of a [`MetricFamily`].
///
/// Carried explicitly so the type is known even for an empty family, rather
/// than inferred from a sample. Mirrors `io.prometheus.client.MetricType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MetricKind {
    /// A monotonically increasing counter.
    Counter,
    /// A gauge that can go up and down.
    Gauge,
    /// A client-side summary with pre-computed quantiles.
    Summary,
    /// A classic or native Prometheus histogram.
    Histogram,
}

/// A named group of metrics that share the same metric name, help text, and
/// value type — corresponding to a single Prometheus `MetricFamily`.
#[derive(Debug)]
#[non_exhaustive]
pub struct MetricFamily {
    /// The sanitized metric name, including any applicable suffix (e.g.
    /// `"http_requests_total"`).
    pub name: String,
    /// The `HELP` description, if one was registered.
    pub help: Option<String>,
    /// The registered unit, if one was provided at registration.
    pub unit: Option<metrics::Unit>,
    /// The Prometheus metric type of this family.
    pub kind: MetricKind,
    /// The individual time-series samples within this family, each
    /// distinguished by its label set.
    pub metrics: Vec<Metric>,
}

/// A single time-series sample: a set of labels and a typed value.
#[derive(Debug)]
#[non_exhaustive]
pub struct Metric {
    /// Labels that identify this particular time series.
    pub labels: Vec<LabelPair>,
    /// The typed metric value.
    pub value: MetricValue,
}

/// The typed payload of a metric sample.
#[derive(Debug)]
#[non_exhaustive]
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
#[non_exhaustive]
pub struct Quantile {
    /// The quantile rank in `[0.0, 1.0]` (e.g. `0.99` for the 99th percentile).
    pub quantile: f64,
    /// The observed value at this quantile.
    pub value: f64,
}

/// A Prometheus summary: pre-computed quantiles plus total count and sum.
#[derive(Debug)]
#[non_exhaustive]
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
#[non_exhaustive]
pub struct Bucket {
    /// Cumulative count of observations that fall at or below [`upper_bound`](Self::upper_bound).
    pub cumulative_count: u64,
    /// The inclusive upper bound of this bucket. The final bucket uses
    /// [`f64::INFINITY`] to represent the `+Inf` boundary.
    pub upper_bound: f64,
}

/// A classic (fixed-bucket) Prometheus histogram.
#[derive(Debug)]
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
    descriptions_rd: DescriptionReadHandle,
    counter_suffix: Option<&'static str>,
) -> RenderedMetrics {
    RenderedMetrics {
        counters: snapshot.counters.into_iter(),
        gauges: snapshot.gauges.into_iter(),
        distributions: snapshot.distributions.into_iter(),
        descriptions_rd,
        counter_suffix,
    }
}

fn render_counter(
    name: &str,
    by_labels: std::collections::HashMap<LabelSet, u64>,
    descriptions_rd: &DescriptionReadHandle,
    counter_suffix: Option<&'static str>,
) -> MetricFamily {
    render_metric(
        name,
        by_labels,
        descriptions_rd,
        counter_suffix,
        MetricKind::Counter,
        MetricValue::Counter,
    )
}

fn render_gauge(
    name: &str,
    by_labels: std::collections::HashMap<LabelSet, f64>,
    descriptions_rd: &DescriptionReadHandle,
) -> MetricFamily {
    render_metric(name, by_labels, descriptions_rd, None, MetricKind::Gauge, MetricValue::Gauge)
}

fn render_distribution(
    name: &str,
    by_labels: indexmap::IndexMap<LabelSet, crate::Distribution>,
    descriptions_rd: &DescriptionReadHandle,
) -> MetricFamily {
    let kind = match by_labels.values().next() {
        Some(crate::Distribution::Summary(..)) => MetricKind::Summary,
        _ => MetricKind::Histogram,
    };
    render_metric(name, by_labels, descriptions_rd, None, kind, render_distribution_value)
}

fn get_help_and_unit(
    name: &str,
    descriptions_rd: &DescriptionReadHandle,
) -> (Option<String>, Option<metrics::Unit>) {
    match descriptions_rd.get_one(name).as_deref() {
        Some((desc, unit)) => {
            let help = Some(desc.clone().into_owned()).filter(|desc| !desc.is_empty());
            // `metrics::Unit` is `Copy`.
            (help, *unit)
        }
        None => (None, None),
    }
}

fn get_labels(labels: LabelSet) -> Vec<LabelPair> {
    labels.labels.into_iter().map(|(label, value)| LabelPair { label, value }).collect()
}

fn render_metric<T>(
    name: &str,
    by_labels: impl IntoIterator<Item = (LabelSet, T)>,
    descriptions_rd: &DescriptionReadHandle,
    counter_suffix: Option<&'static str>,
    kind: MetricKind,
    mut render_value: impl FnMut(T) -> MetricValue,
) -> MetricFamily {
    let (help, unit) = get_help_and_unit(name, descriptions_rd);
    MetricFamily {
        name: add_suffix_to_name(sanitize_metric_name(name), counter_suffix),
        help,
        unit,
        kind,
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
    use std::collections::BTreeMap;
    use std::collections::HashMap;

    use indexmap::IndexMap;

    use super::*;
    use crate::common::Snapshot;
    use crate::distribution::Distribution;
    use crate::recorder::new_description_handles;

    fn buckets(pairs: &[(i32, u64)]) -> BTreeMap<i32, u64> {
        pairs.iter().copied().collect()
    }

    fn label_set(name: &str, k: &str, v: &str) -> LabelSet {
        LabelSet::from_key_and_global(
            &metrics::Key::from_parts(
                String::from(name),
                vec![metrics::Label::new(String::from(k), String::from(v))],
            ),
            &IndexMap::new(),
        )
    }

    fn classic_histogram() -> Distribution {
        let mut dist = Distribution::new_histogram(&[1.0, 5.0, 10.0]);
        let now = quanta::Instant::now();
        dist.record_samples(&[(0.5, now), (2.0, now), (7.0, now)]);
        dist
    }

    #[test]
    fn test_add_suffix_to_name() {
        assert_eq!(add_suffix_to_name("requests".to_owned(), Some("total")), "requests_total");
        assert_eq!(
            add_suffix_to_name("requests_total".to_owned(), Some("total")),
            "requests_total"
        );
        assert_eq!(add_suffix_to_name("requests".to_owned(), None), "requests");
    }

    #[test]
    fn make_buckets_empty() {
        let (spans, deltas) = make_buckets(BTreeMap::new());
        assert!(spans.is_empty());
        assert!(deltas.is_empty());
    }

    #[test]
    fn make_buckets_single() {
        // One bucket at index 2, count 5.
        let (spans, deltas) = make_buckets(buckets(&[(2, 5)]));
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].offset, 2);
        assert_eq!(spans[0].length, 1);
        assert_eq!(deltas, vec![5]);
    }

    #[test]
    fn make_buckets_contiguous() {
        // Indices 0,1,2 with counts 1,2,3 -> deltas are 1,+1,+1.
        let (spans, deltas) = make_buckets(buckets(&[(0, 1), (1, 2), (2, 3)]));
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].offset, 0);
        assert_eq!(spans[0].length, 3);
        assert_eq!(deltas, vec![1, 1, 1]);
    }

    #[test]
    fn make_buckets_small_gap_merged() {
        // Gap of 2 (indices 0 and 3) is bridged with empty buckets in one span,
        // matching the Go encoder. Empty buckets emit -prev_count deltas.
        let (spans, deltas) = make_buckets(buckets(&[(0, 4), (3, 4)]));
        assert_eq!(spans.len(), 1, "gap of <=2 must not create a new span");
        assert_eq!(spans[0].offset, 0);
        assert_eq!(spans[0].length, 4); // bucket 0, two empties, bucket 3
                                        // bucket0: 4-0=4; empty: -4; empty: 0; bucket3: 4-0=4
        assert_eq!(deltas, vec![4, -4, 0, 4]);
    }

    #[test]
    fn make_buckets_large_gap_new_span() {
        // Gap of 3 (indices 0 and 4) creates a new span with offset = gap.
        let (spans, deltas) = make_buckets(buckets(&[(0, 4), (4, 7)]));
        assert_eq!(spans.len(), 2, "gap of >2 must create a new span");
        assert_eq!(spans[0].offset, 0);
        assert_eq!(spans[0].length, 1);
        assert_eq!(spans[1].offset, 4 - 1); // offset from next expected index (1) to 4
        assert_eq!(spans[1].length, 1);
        // prev_count is NOT reset across a new span: bucket0 delta = 4-0 = 4;
        // bucket4 delta = 7 - prev_count(4) = 3.
        assert_eq!(deltas, vec![4, 3]);
    }

    #[test]
    fn make_buckets_negative_indices() {
        let (spans, deltas) = make_buckets(buckets(&[(-2, 3), (-1, 5)]));
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].offset, -2);
        assert_eq!(spans[0].length, 2);
        assert_eq!(deltas, vec![3, 2]); // 3, then 5-3=2
    }

    #[test]
    fn rendered_metrics_yields_counters_then_gauges_then_distributions() {
        let mut counters = HashMap::new();
        counters.insert("c".to_string(), {
            let mut m = HashMap::new();
            m.insert(label_set("c", "k", "v"), 1u64);
            m
        });
        let mut gauges = HashMap::new();
        gauges.insert("g".to_string(), {
            let mut m = HashMap::new();
            m.insert(label_set("g", "k", "v"), 1.0f64);
            m
        });
        let mut distributions = HashMap::new();
        distributions.insert("d".to_string(), {
            let mut m = IndexMap::new();
            m.insert(label_set("d", "k", "v"), classic_histogram());
            m
        });

        let snapshot = Snapshot { counters, gauges, distributions };
        let (mut wr, rd) = new_description_handles();
        wr.publish();

        let kinds: Vec<MetricKind> =
            render_snapshot_and_descriptions(snapshot, rd, Some("total")).map(|f| f.kind).collect();

        assert_eq!(kinds, vec![MetricKind::Counter, MetricKind::Gauge, MetricKind::Histogram]);
    }

    #[test]
    fn gauge_family_carries_kind_and_unit() {
        let mut gauges = HashMap::new();
        gauges.insert("mem_used".to_string(), {
            let mut m = HashMap::new();
            m.insert(label_set("mem_used", "host", "a"), 1.0f64);
            m
        });
        let snapshot = Snapshot { counters: HashMap::new(), gauges, distributions: HashMap::new() };

        let (mut wr, rd) = new_description_handles();
        wr.update(
            "mem_used".to_string(),
            (metrics::SharedString::const_str("Memory used"), Some(metrics::Unit::Bytes)),
        );
        wr.publish();

        let family = render_snapshot_and_descriptions(snapshot, rd, None).next().unwrap();
        assert_eq!(family.kind, MetricKind::Gauge);
        assert_eq!(family.unit, Some(metrics::Unit::Bytes));
        assert_eq!(family.help.as_deref(), Some("Memory used"));
    }

    #[test]
    fn rendered_metrics_is_exact_size() {
        let mut counters = HashMap::new();
        counters.insert("c".to_string(), {
            let mut m = HashMap::new();
            m.insert(label_set("c", "k", "v"), 1u64);
            m
        });
        let mut gauges = HashMap::new();
        gauges.insert("g".to_string(), {
            let mut m = HashMap::new();
            m.insert(label_set("g", "k", "v"), 1.0f64);
            m
        });
        let snapshot = Snapshot { counters, gauges, distributions: HashMap::new() };
        let (mut wr, rd) = new_description_handles();
        wr.publish();

        let mut iter = render_snapshot_and_descriptions(snapshot, rd, Some("total"));
        assert_eq!(iter.len(), 2);
        assert_eq!(iter.size_hint(), (2, Some(2)));
        let _ = iter.next();
        assert_eq!(iter.len(), 1, "len must shrink as items are consumed");
        let _ = iter.next();
        assert_eq!(iter.len(), 0);
        assert!(iter.next().is_none());
    }
}
