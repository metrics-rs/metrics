//! Render metrics into structured objects instead of serializing as text or protobuf.

use crate::common::Snapshot;
use crate::formatting::sanitize_metric_name;
use crate::recorder::DescriptionReadHandle;
use crate::LabelSet;

#[expect(missing_docs)]
pub type RenderedMetrics = Vec<MetricFamily>;

#[derive(Debug)]
#[expect(missing_docs)]
pub struct LabelPair {
    pub label: String,
    pub value: String,
}

#[derive(Debug)]
#[expect(missing_docs)]
pub struct MetricFamily {
    pub name: String,
    pub help: Option<String>,
    pub metrics: Vec<Metric>,
}

#[derive(Debug)]
#[expect(missing_docs)]
pub struct Metric {
    pub labels: Vec<LabelPair>,
    pub value: MetricValue,
}

#[derive(Debug)]
#[expect(missing_docs)]
pub enum MetricValue {
    Counter(u64),
    Gauge(f64),
    Summary(Summary),
    ClassicHistogram(ClassicHistogram),
    NativeHistogram(NativeHistogram),
}

#[derive(Debug)]
#[expect(missing_docs)]
pub struct Quantile {
    pub quantile: f64,
    pub value: f64,
}

#[derive(Debug)]
#[expect(missing_docs)]
pub struct Summary {
    pub sample_count: u64,
    pub sample_sume: f64,
    pub quantiles: Vec<Quantile>,
}

#[derive(Debug)]
#[expect(missing_docs)]
pub struct Bucket {
    pub cumulative_count: u64,
    pub upper_bound: f64,
}

#[derive(Debug)]
#[expect(missing_docs)]
pub struct ClassicHistogram {
    pub sample_count: u64,
    pub sample_sum: f64,
    pub buckets: Vec<Bucket>,
}

#[derive(Debug)]
#[expect(missing_docs)]
pub struct BucketSpan {
    pub offset: i32,
    pub length: u32,
}

#[derive(Debug)]
#[expect(missing_docs)]
pub struct NativeHistogram {
    pub sample_count: u64,
    pub sample_sum: f64,
    pub zero_threshold: f64,
    pub schema: i32,
    pub zero_count: u64,

    pub positive_spans: Vec<BucketSpan>,
    pub positive_deltas: Vec<i64>,

    pub negative_spans: Vec<BucketSpan>,
    pub negative_deltas: Vec<i64>,
}

pub(crate) fn render_snapshot_and_descriptions(
    snapshot: Snapshot,
    descriptions_rd: &DescriptionReadHandle,
    counter_suffix: Option<&'static str>,
) -> RenderedMetrics {
    let counters = snapshot.counters.into_iter().map(|(name, by_labels)| {
        render_metric(&name, by_labels, descriptions_rd, counter_suffix, MetricValue::Counter)
    });

    let gauges = snapshot.gauges.into_iter().map(|(name, by_labels)| {
        render_metric(&name, by_labels, descriptions_rd, None, MetricValue::Gauge)
    });

    let distributions = snapshot.distributions.into_iter().map(|(name, by_labels)| {
        render_metric(&name, by_labels, descriptions_rd, None, render_distribution)
    });

    counters.chain(gauges).chain(distributions).collect()
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
    let help = get_help(name, descriptions_rd);
    let metrics: Vec<_> = by_labels
        .into_iter()
        .map(|(labels, value)| Metric { labels: get_labels(labels), value: render_value(value) })
        .collect();
    let name = sanitize_metric_name(name);
    let is_counter = matches!(metrics.first(), Some(Metric { value: MetricValue::Counter(_), .. }));
    let name = if is_counter { add_suffix_to_name(name, counter_suffix) } else { name };
    MetricFamily { name, help, metrics }
}

fn add_suffix_to_name(name: String, suffix: Option<&'static str>) -> String {
    match suffix {
        Some(suffix) if !name.ends_with(suffix) => format!("{name}_{suffix}"),
        _ => name,
    }
}

#[expect(clippy::needless_pass_by_value, reason = "matches FnMut signature")]
fn render_distribution(distribution: crate::Distribution) -> MetricValue {
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
        sample_sume: sum,
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
        #[allow(clippy::cast_possible_wrap)]
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
