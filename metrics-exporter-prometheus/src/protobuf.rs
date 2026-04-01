//! Protobuf serialization support for Prometheus metrics.

use prost::Message;
use std::io::Write;

use crate::common::Snapshot;
use crate::recorder::DescriptionReadHandle;

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
pub(crate) fn render_protobuf(
    snapshot: Snapshot,
    descriptions_rd: &DescriptionReadHandle,
    counter_suffix: Option<&'static str>,
) -> Vec<u8> {
    let mut output = Vec::new();
    render_protobuf_to_write(&mut output, snapshot, descriptions_rd, counter_suffix)
        .expect("writing to an in-memory buffer should not fail");
    output
}

/// Renders a snapshot of metrics into protobuf format using length-delimited encoding.
///
/// This function takes a snapshot of metrics and converts them into the Prometheus
/// protobuf wire format, where each `MetricFamily` message is prefixed with a varint
/// length header.
pub(crate) fn render_protobuf_to_write<W: Write>(
    writer: &mut W,
    snapshot: Snapshot,
    descriptions_rd: &DescriptionReadHandle,
    counter_suffix: Option<&'static str>,
) -> std::io::Result<()> {
    let mut buffer = Vec::new();

    // Process counters
    for (name, by_labels) in snapshot.counters {
        let metric_family =
            crate::render::render_counter(&name, by_labels, descriptions_rd, counter_suffix)
                .into_protobuf();
        buffer.clear();
        metric_family.encode_length_delimited(&mut buffer).unwrap();
        writer.write_all(&buffer)?;
    }

    // Process gauges
    for (name, by_labels) in snapshot.gauges {
        let metric_family =
            crate::render::render_gauge(&name, by_labels, descriptions_rd).into_protobuf();
        buffer.clear();
        metric_family.encode_length_delimited(&mut buffer).unwrap();
        writer.write_all(&buffer)?;
    }

    // Process distributions (histograms and summaries)
    for (name, by_labels) in snapshot.distributions {
        let metric_family =
            crate::render::render_distribution(&name, by_labels, descriptions_rd).into_protobuf();
        buffer.clear();
        metric_family.encode_length_delimited(&mut buffer).unwrap();
        writer.write_all(&buffer)?;
    }

    Ok(())
}

impl crate::render::MetricFamily {
    fn into_protobuf(self) -> pb::MetricFamily {
        let metric_type = self.metrics.first().map(|m| m.protobuf_metric_type() as i32);
        pb::MetricFamily {
            name: Some(self.name),
            help: self.help,
            r#type: metric_type,
            metric: self.metrics.into_iter().map(crate::render::Metric::into_protobuf).collect(),
            unit: None,
        }
    }
}

impl crate::render::Metric {
    const fn protobuf_metric_type(&self) -> pb::MetricType {
        use crate::render::MetricValue::{
            ClassicHistogram, Counter, Gauge, NativeHistogram, Summary,
        };
        match self.value {
            Counter(_) => pb::MetricType::Counter,
            Gauge(_) => pb::MetricType::Gauge,
            Summary(_) => pb::MetricType::Summary,
            ClassicHistogram(_) | NativeHistogram(_) => pb::MetricType::Histogram,
        }
    }

    fn into_protobuf(self) -> pb::Metric {
        let mut metric = pb::Metric {
            label: self
                .labels
                .into_iter()
                .map(|crate::render::LabelPair { label, value }| pb::LabelPair {
                    name: Some(label),
                    value: Some(value),
                })
                .collect(),
            ..Default::default()
        };

        match self.value.into_protobuf() {
            ProtobufMetricValue::Counter(counter) => metric.counter = Some(counter),
            ProtobufMetricValue::Gauge(gauge) => metric.gauge = Some(gauge),
            ProtobufMetricValue::Summary(summary) => metric.summary = Some(summary),
            ProtobufMetricValue::Histogram(histogram) => metric.histogram = Some(histogram),
        }

        metric
    }
}

impl crate::render::MetricValue {
    fn into_protobuf(self) -> ProtobufMetricValue {
        use crate::render::MetricValue::{
            ClassicHistogram, Counter, Gauge, NativeHistogram, Summary,
        };
        match self {
            Counter(value) => ProtobufMetricValue::Counter(pb::Counter {
                #[expect(clippy::cast_precision_loss)]
                value: Some(value as f64),
                ..Default::default()
            }),
            Gauge(value) => ProtobufMetricValue::Gauge(pb::Gauge { value: Some(value) }),
            Summary(summary) => ProtobufMetricValue::Summary(summary.into_protobuf()),
            ClassicHistogram(histogram) => {
                ProtobufMetricValue::Histogram(histogram.into_protobuf())
            }
            NativeHistogram(native_histogram) => {
                ProtobufMetricValue::Histogram(native_histogram.into_protobuf())
            }
        }
    }
}

impl crate::render::Summary {
    fn into_protobuf(self) -> pb::Summary {
        pb::Summary {
            sample_count: Some(self.sample_count),
            sample_sum: Some(self.sample_sum),
            quantile: self
                .quantiles
                .into_iter()
                .map(|q| pb::Quantile { quantile: Some(q.quantile), value: Some(q.value) })
                .collect(),
            created_timestamp: None,
        }
    }
}

impl crate::render::ClassicHistogram {
    fn into_protobuf(self) -> pb::Histogram {
        pb::Histogram {
            sample_count: Some(self.sample_count),
            sample_sum: Some(self.sample_sum),
            bucket: self
                .buckets
                .into_iter()
                .map(|crate::render::Bucket { cumulative_count, upper_bound }| pb::Bucket {
                    cumulative_count: Some(cumulative_count),
                    upper_bound: Some(upper_bound),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }
}

impl crate::render::NativeHistogram {
    fn into_protobuf(self) -> pb::Histogram {
        pb::Histogram {
            sample_count: Some(self.sample_count),
            sample_sum: Some(self.sample_sum),
            zero_threshold: Some(self.zero_threshold),
            schema: Some(self.schema),
            zero_count: Some(self.zero_count),
            positive_span: self
                .positive_spans
                .into_iter()
                .map(|crate::render::BucketSpan { offset, length }| pb::BucketSpan {
                    offset: Some(offset),
                    length: Some(length),
                })
                .collect(),
            positive_delta: self.positive_deltas,
            negative_span: self
                .negative_spans
                .into_iter()
                .map(|crate::render::BucketSpan { offset, length }| pb::BucketSpan {
                    offset: Some(offset),
                    length: Some(length),
                })
                .collect(),
            negative_delta: self.negative_deltas,
            ..Default::default()
        }
    }
}

#[expect(clippy::large_enum_variant, reason = "enum is inlined into intermediate callsites")]
enum ProtobufMetricValue {
    Counter(pb::Counter),
    Gauge(pb::Gauge),
    Summary(pb::Summary),
    Histogram(pb::Histogram),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::Snapshot;
    use crate::recorder::new_description_handles;
    use crate::LabelSet;
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

        let (mut descriptions_wr, descriptions_rd) = new_description_handles();
        descriptions_wr.publish();

        let protobuf_data = render_protobuf(snapshot, &descriptions_rd, Some("total"));

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

        let (mut descriptions_wr, descriptions_rd) = new_description_handles();
        descriptions_wr.update(
            "cpu_usage".to_string(),
            (SharedString::const_str("CPU usage percentage"), None),
        );
        descriptions_wr.publish();

        let protobuf_data = render_protobuf(snapshot, &descriptions_rd, None);

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
}
