//! Protobuf serialization support for Prometheus metrics.

use indexmap::IndexMap;
use metrics::Unit;
use prost::Message;
use std::collections::HashMap;

use crate::common::Snapshot;
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
    snapshot: &Snapshot,
    descriptions: &HashMap<String, (metrics::SharedString, Option<Unit>)>,
    global_labels: &IndexMap<String, String>,
    counter_suffix: Option<&'static str>,
) -> Vec<u8> {
    let mut output = Vec::new();

    // Process counters
    for (name, by_labels) in &snapshot.counters {
        let sanitized_name = sanitize_metric_name(name);
        let help =
            descriptions.get(name.as_str()).map(|(desc, _)| desc.to_string()).unwrap_or_default();

        let mut metrics = Vec::new();
        for (labels, value) in by_labels {
            let label_pairs = parse_labels(labels, global_labels);

            metrics.push(pb::Metric {
                label: label_pairs,
                counter: Some(pb::Counter {
                    #[allow(clippy::cast_precision_loss)]
                    value: Some(*value as f64),

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
    for (name, by_labels) in &snapshot.gauges {
        let sanitized_name = sanitize_metric_name(name);
        let help =
            descriptions.get(name.as_str()).map(|(desc, _)| desc.to_string()).unwrap_or_default();

        let mut metrics = Vec::new();
        for (labels, value) in by_labels {
            let label_pairs = parse_labels(labels, global_labels);

            metrics.push(pb::Metric {
                label: label_pairs,
                gauge: Some(pb::Gauge { value: Some(*value) }),

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
    for (name, by_labels) in &snapshot.distributions {
        let sanitized_name = sanitize_metric_name(name);
        let help =
            descriptions.get(name.as_str()).map(|(desc, _)| desc.to_string()).unwrap_or_default();

        let mut metrics = Vec::new();
        for (labels, distribution) in by_labels {
            let label_pairs = parse_labels(labels, global_labels);

            let metric = match distribution {
                Distribution::Summary(summary, quantiles, sum) => {
                    use quanta::Instant;
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
                            sample_sum: Some(*sum),
                            quantile: quantile_values,

                            created_timestamp: None,
                        }),

                        ..Default::default()
                    }
                }
                Distribution::Histogram(histogram) => {
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
            };

            metrics.push(metric);
        }

        let metric_type = match by_labels.values().next() {
            Some(Distribution::Summary(_, _, _)) => pb::MetricType::Summary,
            Some(Distribution::Histogram(_)) => pb::MetricType::Histogram,
            None => continue, // Skip empty metric families
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

fn parse_labels(labels: &[String], global_labels: &IndexMap<String, String>) -> Vec<pb::LabelPair> {
    let mut label_pairs = Vec::new();

    // Add global labels first
    for (key, value) in global_labels {
        label_pairs.push(pb::LabelPair { name: Some(key.clone()), value: Some(value.clone()) });
    }

    // Add metric-specific labels
    for label_str in labels {
        if let Some(eq_pos) = label_str.find('=') {
            let key = &label_str[..eq_pos];
            let value = &label_str[eq_pos + 1..];
            let value = value.trim_matches('"');

            // Skip if this label key already exists from global labels
            if !global_labels.contains_key(key) {
                label_pairs.push(pb::LabelPair {
                    name: Some(key.to_string()),
                    value: Some(value.to_string()),
                });
            }
        }
    }

    label_pairs
}

fn add_suffix_to_name(name: &str, suffix: Option<&'static str>) -> String {
    match suffix {
        Some(suffix) if !name.ends_with(suffix) => format!("{name}_{suffix}"),
        _ => name.to_string(),
    }
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
        counter_labels.insert(vec!["method=\"GET\"".to_string()], 42u64);
        counters.insert("http_requests".to_string(), counter_labels);

        let snapshot = Snapshot { counters, gauges: HashMap::new(), distributions: HashMap::new() };

        let descriptions = HashMap::new();
        let global_labels = IndexMap::new();

        let protobuf_data =
            render_protobuf(&snapshot, &descriptions, &global_labels, Some("total"));

        assert!(!protobuf_data.is_empty(), "Protobuf data should not be empty");

        // Parse the protobuf response to verify it's correct
        let metric_family = pb::MetricFamily::decode_length_delimited(&protobuf_data[..]).unwrap();

        assert_eq!(metric_family.name.as_ref().unwrap(), "http_requests_total");
        assert_eq!(metric_family.r#type.unwrap(), pb::MetricType::Counter as i32);
        assert_eq!(metric_family.metric.len(), 1);

        let metric = &metric_family.metric[0];
        assert!(metric.counter.is_some());
        assert_eq!(metric.counter.as_ref().unwrap().value.unwrap(), 42.0);
    }

    #[test]
    fn test_render_protobuf_gauges() {
        let mut gauges = HashMap::new();
        let mut gauge_labels = HashMap::new();
        gauge_labels.insert(vec!["instance=\"localhost\"".to_string()], 0.75f64);
        gauges.insert("cpu_usage".to_string(), gauge_labels);

        let snapshot = Snapshot { counters: HashMap::new(), gauges, distributions: HashMap::new() };

        let mut descriptions = HashMap::new();
        descriptions.insert(
            "cpu_usage".to_string(),
            (SharedString::const_str("CPU usage percentage"), None),
        );
        let global_labels = IndexMap::new();

        let protobuf_data = render_protobuf(&snapshot, &descriptions, &global_labels, None);

        assert!(!protobuf_data.is_empty(), "Protobuf data should not be empty");

        // Parse the protobuf response to verify it's correct
        let metric_family = pb::MetricFamily::decode_length_delimited(&protobuf_data[..]).unwrap();

        assert_eq!(metric_family.name.as_ref().unwrap(), "cpu_usage");
        assert_eq!(metric_family.r#type.unwrap(), pb::MetricType::Gauge as i32);
        assert_eq!(metric_family.help.as_ref().unwrap(), "CPU usage percentage");

        let metric = &metric_family.metric[0];
        assert!(metric.gauge.is_some());
        assert_eq!(metric.gauge.as_ref().unwrap().value.unwrap(), 0.75);
    }

    #[test]
    fn test_add_suffix_to_name() {
        assert_eq!(add_suffix_to_name("requests", Some("total")), "requests_total");
        assert_eq!(add_suffix_to_name("requests_total", Some("total")), "requests_total");
        assert_eq!(add_suffix_to_name("requests", None), "requests");
    }
}
