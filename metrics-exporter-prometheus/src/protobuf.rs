//! Protobuf serialization support for Prometheus metrics.

use metrics::Unit;
use prost::Message;
use std::collections::HashMap;

use crate::common::{LabelSet, Snapshot};
use crate::distribution::Distribution;
use crate::formatting::sanitize_metric_name;

// Include the generated protobuf code
mod pb {
    #![allow(missing_docs, clippy::trivially_copy_pass_by_ref, clippy::doc_markdown)]
    include!(concat!(env!("OUT_DIR"), "/io.prometheus.client.rs"));
}

#[cfg(feature = "http-listener")]
pub(crate) const PROTOBUF_CONTENT_TYPE: &str =
    "application/vnd.google.protobuf; proto=io.prometheus.client.MetricFamily; encoding=delimited";

/// Renders a snapshot of metrics into protobuf format using length-delimited encoding.
///
/// This function takes a snapshot of metrics and converts them into the Prometheus
/// protobuf wire format, where each `MetricFamily` message is prefixed with a varint
/// length header.
#[allow(clippy::too_many_lines)]
pub(crate) fn render_protobuf(
    snapshot: Snapshot,
    descriptions: &HashMap<String, (metrics::SharedString, Option<Unit>)>,
    counter_suffix: Option<&'static str>,
) -> Vec<u8> {
    let mut output = Vec::new();

    // Process counters
    for (name, by_labels) in snapshot.counters {
        let sanitized_name = sanitize_metric_name(&name);
        let help =
            descriptions.get(name.as_str()).map(|(desc, _)| desc.to_string()).unwrap_or_default();

        let mut metrics = Vec::new();
        for (labels, value) in by_labels {
            let label_pairs = label_set_to_protobuf(labels);

            metrics.push(pb::Metric {
                label: label_pairs,
                counter: Some(pb::Counter {
                    #[allow(clippy::cast_precision_loss)]
                    value: Some(value as f64),

                    ..Default::default()
                }),

                ..Default::default()
            });
        }

        let metric_family = pb::MetricFamily {
            name: Some(add_suffix_to_name(&sanitized_name, counter_suffix)),
            help: if help.is_empty() { None } else { Some(help) },
            r#type: Some(pb::MetricType::Counter as i32),
            metric: metrics,
            unit: None,
        };

        metric_family.encode_length_delimited(&mut output).unwrap();
    }

    // Process gauges
    for (name, by_labels) in snapshot.gauges {
        let sanitized_name = sanitize_metric_name(&name);
        let help =
            descriptions.get(name.as_str()).map(|(desc, _)| desc.to_string()).unwrap_or_default();

        let mut metrics = Vec::new();
        for (labels, value) in by_labels {
            let label_pairs = label_set_to_protobuf(labels);

            metrics.push(pb::Metric {
                label: label_pairs,
                gauge: Some(pb::Gauge { value: Some(value) }),

                ..Default::default()
            });
        }

        let metric_family = pb::MetricFamily {
            name: Some(sanitized_name),
            help: if help.is_empty() { None } else { Some(help) },
            r#type: Some(pb::MetricType::Gauge as i32),
            metric: metrics,
            unit: None,
        };

        metric_family.encode_length_delimited(&mut output).unwrap();
    }

    // Process distributions (histograms and summaries)
    for (name, by_labels) in snapshot.distributions {
        let sanitized_name = sanitize_metric_name(&name);
        let help =
            descriptions.get(name.as_str()).map(|(desc, _)| desc.to_string()).unwrap_or_default();

        let mut metrics = Vec::new();
        let mut metric_type = None;
        for (labels, distribution) in by_labels {
            let label_pairs = label_set_to_protobuf(labels);

            let metric = match distribution {
                Distribution::Summary(summary, quantiles, sum) => {
                    use quanta::Instant;
                    metric_type = Some(pb::MetricType::Summary);
                    let snapshot = summary.snapshot(Instant::now());
                    let quantile_values: Vec<pb::Quantile> = quantiles
                        .iter()
                        .map(|q| pb::Quantile {
                            quantile: Some(q.value()),
                            value: Some(snapshot.quantile(q.value()).unwrap_or(0.0)),
                        })
                        .collect();

                    pb::Metric {
                        label: label_pairs,
                        summary: Some(pb::Summary {
                            sample_count: Some(summary.count() as u64),
                            sample_sum: Some(sum),
                            quantile: quantile_values,

                            created_timestamp: None,
                        }),

                        ..Default::default()
                    }
                }
                Distribution::Histogram(histogram) => {
                    metric_type = Some(pb::MetricType::Histogram);
                    let mut buckets = Vec::new();
                    for (le, count) in histogram.buckets() {
                        buckets.push(pb::Bucket {
                            cumulative_count: Some(count),
                            upper_bound: Some(le),

                            ..Default::default()
                        });
                    }
                    // Add +Inf bucket
                    buckets.push(pb::Bucket {
                        cumulative_count: Some(histogram.count()),
                        upper_bound: Some(f64::INFINITY),

                        ..Default::default()
                    });

                    pb::Metric {
                        label: label_pairs,
                        histogram: Some(pb::Histogram {
                            sample_count: Some(histogram.count()),
                            sample_sum: Some(histogram.sum()),
                            bucket: buckets,

                            ..Default::default()
                        }),

                        ..Default::default()
                    }
                }
                Distribution::NativeHistogram(native_hist) => {
                    metric_type = Some(pb::MetricType::Histogram);
                    // Convert our native histogram into Prometheus native histogram format
                    let positive_buckets = native_hist.positive_buckets();
                    let negative_buckets = native_hist.negative_buckets();

                    // Get the current schema being used by the histogram
                    let schema = native_hist.schema();

                    // Convert positive buckets to spans and deltas (matches Go makeBuckets function)
                    let (positive_spans, positive_deltas) = make_buckets(&positive_buckets);
                    let (negative_spans, negative_deltas) = make_buckets(&negative_buckets);

                    // Match Go Write() method output exactly
                    let mut histogram = pb::Histogram {
                        sample_count: Some(native_hist.count()),
                        sample_sum: Some(native_hist.sum()),

                        // Native histogram fields from Go implementation
                        zero_threshold: Some(native_hist.config().zero_threshold()),
                        schema: Some(schema),
                        zero_count: Some(native_hist.zero_count()),

                        positive_span: positive_spans,
                        positive_delta: positive_deltas,

                        negative_span: negative_spans,
                        negative_delta: negative_deltas,

                        ..Default::default()
                    };

                    // Add a no-op span if histogram is empty (matches Go implementation)
                    if histogram.zero_threshold == Some(0.0)
                        && histogram.zero_count == Some(0)
                        && histogram.positive_span.is_empty()
                        && histogram.negative_span.is_empty()
                    {
                        histogram.positive_span =
                            vec![pb::BucketSpan { offset: Some(0), length: Some(0) }];
                    }

                    pb::Metric {
                        label: label_pairs,
                        histogram: Some(histogram),
                        ..Default::default()
                    }
                }
            };

            metrics.push(metric);
        }

        let Some(metric_type) = metric_type else {
            // Skip empty metric families
            continue;
        };

        let metric_family = pb::MetricFamily {
            name: Some(sanitized_name),
            help: if help.is_empty() { None } else { Some(help) },
            r#type: Some(metric_type as i32),
            metric: metrics,
            unit: None,
        };

        metric_family.encode_length_delimited(&mut output).unwrap();
    }

    output
}

fn label_set_to_protobuf(labels: LabelSet) -> Vec<pb::LabelPair> {
    let mut label_pairs = Vec::new();

    for (key, value) in labels.labels {
        label_pairs.push(pb::LabelPair { name: Some(key), value: Some(value) });
    }

    label_pairs
}

fn add_suffix_to_name(name: &str, suffix: Option<&'static str>) -> String {
    match suffix {
        Some(suffix) if !name.ends_with(suffix) => format!("{name}_{suffix}"),
        _ => name.to_string(),
    }
}

/// Convert a `BTreeMap` of bucket indices to counts into Prometheus native histogram
/// spans and deltas format. This follows the Go `makeBucketsFromMap` function.
fn make_buckets(buckets: &std::collections::BTreeMap<i32, u64>) -> (Vec<pb::BucketSpan>, Vec<i64>) {
    if buckets.is_empty() {
        return (vec![], vec![]);
    }

    // Get sorted bucket indices (similar to Go's sorting)
    let mut indices: Vec<i32> = buckets.keys().copied().collect();
    indices.sort_unstable();

    let mut spans = Vec::new();
    let mut deltas = Vec::new();
    let mut prev_count = 0i64;
    let mut next_i = 0i32;

    for (n, &i) in indices.iter().enumerate() {
        #[allow(clippy::cast_possible_wrap)]
        let count = buckets[&i] as i64;

        // Multiple spans with only small gaps in between are probably
        // encoded more efficiently as one larger span with a few empty buckets.
        // Following Go: gaps of one or two buckets should not create a new span.
        let i_delta = i - next_i;

        if n == 0 || i_delta > 2 {
            // Create a new span - either first bucket or gap > 2
            spans.push(pb::BucketSpan { offset: Some(i_delta), length: Some(0) });
        } else {
            // Small gap (or no gap) - insert empty buckets as needed
            for _ in 0..i_delta {
                if let Some(last_span) = spans.last_mut() {
                    *last_span.length.as_mut().unwrap() += 1;
                }
                deltas.push(-prev_count);
                prev_count = 0;
            }
        }

        // Add the current bucket
        if let Some(last_span) = spans.last_mut() {
            *last_span.length.as_mut().unwrap() += 1;
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
    use crate::common::Snapshot;
    use indexmap::IndexMap;
    use metrics::SharedString;
    use prost::Message;
    use std::collections::HashMap;

    #[test]
    fn test_render_protobuf_counters() {
        let mut counters = HashMap::new();
        let mut counter_labels = HashMap::new();
        let labels = LabelSet::from_key_and_global(
            &metrics::Key::from_parts("", vec![metrics::Label::new("method", "GET")]),
            &IndexMap::new(),
        );
        counter_labels.insert(labels, 42u64);
        counters.insert("http_requests".to_string(), counter_labels);

        let snapshot = Snapshot { counters, gauges: HashMap::new(), distributions: HashMap::new() };

        let descriptions = HashMap::new();

        let protobuf_data = render_protobuf(snapshot, &descriptions, Some("total"));

        assert!(!protobuf_data.is_empty(), "Protobuf data should not be empty");

        // Parse the protobuf response to verify it's correct
        let metric_family = pb::MetricFamily::decode_length_delimited(&protobuf_data[..]).unwrap();

        assert_eq!(metric_family.name.as_ref().unwrap(), "http_requests_total");
        assert_eq!(metric_family.r#type.unwrap(), pb::MetricType::Counter as i32);
        assert_eq!(metric_family.metric.len(), 1);

        let metric = &metric_family.metric[0];
        assert!(metric.counter.is_some());
        let counter_value = metric.counter.as_ref().unwrap().value.unwrap();
        assert!((counter_value - 42.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_render_protobuf_gauges() {
        let mut gauges = HashMap::new();
        let mut gauge_labels = HashMap::new();
        let labels = LabelSet::from_key_and_global(
            &metrics::Key::from_parts("", vec![metrics::Label::new("instance", "localhost")]),
            &IndexMap::new(),
        );
        gauge_labels.insert(labels, 0.75f64);
        gauges.insert("cpu_usage".to_string(), gauge_labels);

        let snapshot = Snapshot { counters: HashMap::new(), gauges, distributions: HashMap::new() };

        let mut descriptions = HashMap::new();
        descriptions.insert(
            "cpu_usage".to_string(),
            (SharedString::const_str("CPU usage percentage"), None),
        );

        let protobuf_data = render_protobuf(snapshot, &descriptions, None);

        assert!(!protobuf_data.is_empty(), "Protobuf data should not be empty");

        // Parse the protobuf response to verify it's correct
        let metric_family = pb::MetricFamily::decode_length_delimited(&protobuf_data[..]).unwrap();

        assert_eq!(metric_family.name.as_ref().unwrap(), "cpu_usage");
        assert_eq!(metric_family.r#type.unwrap(), pb::MetricType::Gauge as i32);
        assert_eq!(metric_family.help.as_ref().unwrap(), "CPU usage percentage");

        let metric = &metric_family.metric[0];
        assert!(metric.gauge.is_some());
        let gauge_value = metric.gauge.as_ref().unwrap().value.unwrap();
        assert!((gauge_value - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn test_add_suffix_to_name() {
        assert_eq!(add_suffix_to_name("requests", Some("total")), "requests_total");
        assert_eq!(add_suffix_to_name("requests_total", Some("total")), "requests_total");
        assert_eq!(add_suffix_to_name("requests", None), "requests");
    }
}
