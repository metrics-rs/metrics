//! Protobuf serialization support for Prometheus metrics.

use prost::Message;
use std::io::Write;

// Include the generated protobuf code
mod pb {
    #![allow(missing_docs, clippy::trivially_copy_pass_by_ref, clippy::doc_markdown)]
    include!(concat!(env!("OUT_DIR"), "/io.prometheus.client.rs"));
}

#[cfg(feature = "http-listener")]
pub(crate) const PROTOBUF_CONTENT_TYPE: &str =
    "application/vnd.google.protobuf; proto=io.prometheus.client.MetricFamily; encoding=delimited";

/// Encodes pre-rendered metrics into protobuf format using length-delimited encoding.
///
/// Consumes a [`RenderedMetrics`](crate::render::RenderedMetrics) iterator and produces the
/// Prometheus protobuf wire format, where each `MetricFamily` message is prefixed with a varint
/// length header.
pub(crate) fn render_protobuf(rendered_metrics: crate::render::RenderedMetrics) -> Vec<u8> {
    let mut output = Vec::new();
    render_protobuf_to_write(&mut output, rendered_metrics)
        .expect("writing to an in-memory buffer should not fail");
    output
}

/// Encodes pre-rendered metrics into protobuf format and writes them to `writer`.
///
/// Consumes a [`RenderedMetrics`](crate::render::RenderedMetrics) iterator and writes the
/// Prometheus protobuf wire format, where each `MetricFamily` message is prefixed with a varint
/// length header.
pub(crate) fn render_protobuf_to_write<W: Write>(
    writer: &mut W,
    rendered_metrics: crate::render::RenderedMetrics,
) -> std::io::Result<()> {
    let mut buffer = Vec::new();

    for metric_family in rendered_metrics {
        if metric_family.metrics.is_empty() {
            continue;
        }
        let metric_family = metric_family.into_protobuf();
        buffer.clear();
        metric_family.encode_length_delimited(&mut buffer).unwrap();
        writer.write_all(&buffer)?;
    }

    Ok(())
}

impl crate::render::MetricFamily {
    fn into_protobuf(self) -> pb::MetricFamily {
        pb::MetricFamily {
            name: Some(self.name),
            help: self.help,
            r#type: Some(self.kind.into_protobuf() as i32),
            metric: self.metrics.into_iter().map(crate::render::Metric::into_protobuf).collect(),
            unit: None,
        }
    }
}

impl crate::render::MetricKind {
    const fn into_protobuf(self) -> pb::MetricType {
        use crate::render::MetricKind::{Counter, Gauge, Histogram, Summary};
        match self {
            Counter => pb::MetricType::Counter,
            Gauge => pb::MetricType::Gauge,
            Summary => pb::MetricType::Summary,
            Histogram => pb::MetricType::Histogram,
        }
    }
}

impl crate::render::Metric {
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

    fn single_dist(name: &str, dist: crate::distribution::Distribution) -> Snapshot {
        let labels = LabelSet::from_key_and_global(
            &metrics::Key::from_parts(String::from(name), vec![metrics::Label::new("k", "v")]),
            &IndexMap::new(),
        );
        let mut by_labels = IndexMap::new();
        by_labels.insert(labels, dist);
        let mut distributions = HashMap::new();
        distributions.insert(name.to_string(), by_labels);
        Snapshot { counters: HashMap::new(), gauges: HashMap::new(), distributions }
    }

    fn decode_one(data: &[u8]) -> pb::MetricFamily {
        pb::MetricFamily::decode_length_delimited(data).unwrap()
    }

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

        let rendered_metrics = crate::render::render_snapshot_and_descriptions(
            snapshot,
            descriptions_rd,
            Some("total"),
        );
        let protobuf_data = render_protobuf(rendered_metrics);

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
    fn empty_distribution_family_is_skipped() {
        let mut distributions: HashMap<String, IndexMap<LabelSet, crate::distribution::Distribution>> = HashMap::new();
        // A named distribution family with no series.
        distributions.insert("empty_hist".to_string(), IndexMap::new());

        let snapshot =
            Snapshot { counters: HashMap::new(), gauges: HashMap::new(), distributions };
        let (mut wr, rd) = new_description_handles();
        wr.publish();

        let rendered = crate::render::render_snapshot_and_descriptions(snapshot, rd, None);
        let data = render_protobuf(rendered);

        assert!(data.is_empty(), "an empty family must not be encoded");
    }

    #[test]
    fn test_render_protobuf_classic_histogram() {
        let mut dist = crate::distribution::Distribution::new_histogram(&[1.0, 5.0, 10.0]);
        let now = quanta::Instant::now();
        dist.record_samples(&[(0.5, now), (2.0, now), (7.0, now), (20.0, now)]);

        let snapshot = single_dist("req_latency", dist);
        let (mut wr, rd) = new_description_handles();
        wr.publish();

        let data = render_protobuf(crate::render::render_snapshot_and_descriptions(snapshot, rd, None));
        let family = decode_one(&data);

        assert_eq!(family.name.as_deref(), Some("req_latency"));
        assert_eq!(family.r#type.unwrap(), pb::MetricType::Histogram as i32);
        let hist = family.metric[0].histogram.as_ref().unwrap();
        assert_eq!(hist.sample_count, Some(4));
        // Bounded buckets (1,5,10) plus the trailing +Inf bucket.
        assert_eq!(hist.bucket.len(), 4);
        assert_eq!(hist.bucket.last().unwrap().upper_bound, Some(f64::INFINITY));
        assert_eq!(hist.bucket.last().unwrap().cumulative_count, Some(4));
    }

    #[test]
    fn test_render_protobuf_summary() {
        use std::num::NonZeroU32;
        use std::sync::Arc;
        use std::time::Duration;

        let quantiles = Arc::new(metrics_util::parse_quantiles(&[0.5, 0.9]));
        let mut dist = crate::distribution::Distribution::new_summary(
            quantiles,
            Duration::from_secs(60),
            NonZeroU32::new(3).unwrap(),
        );
        let now = quanta::Instant::now();
        dist.record_samples(&[(1.0, now), (2.0, now), (3.0, now), (4.0, now)]);

        let snapshot = single_dist("op_seconds", dist);
        let (mut wr, rd) = new_description_handles();
        wr.publish();

        let data = render_protobuf(crate::render::render_snapshot_and_descriptions(snapshot, rd, None));
        let family = decode_one(&data);

        assert_eq!(family.r#type.unwrap(), pb::MetricType::Summary as i32);
        let summary = family.metric[0].summary.as_ref().unwrap();
        assert_eq!(summary.sample_count, Some(4));
        assert!((summary.sample_sum.unwrap() - 10.0).abs() < f64::EPSILON);
        assert_eq!(summary.quantile.len(), 2);
        assert_eq!(summary.quantile[0].quantile, Some(0.5));
    }

    #[test]
    fn test_render_protobuf_native_histogram_empty_has_noop_span() {
        let config = crate::native_histogram::NativeHistogramConfig::new(2.0, 160, 0.0).unwrap();
        let dist = crate::distribution::Distribution::new_native_histogram(config);

        let snapshot = single_dist("native_empty", dist);
        let (mut wr, rd) = new_description_handles();
        wr.publish();

        let data = render_protobuf(crate::render::render_snapshot_and_descriptions(snapshot, rd, None));
        let family = decode_one(&data);

        assert_eq!(family.r#type.unwrap(), pb::MetricType::Histogram as i32);
        let hist = family.metric[0].histogram.as_ref().unwrap();
        // Empty native histogram emits a single no-op positive span.
        assert_eq!(hist.positive_span.len(), 1);
        assert_eq!(hist.positive_span[0].offset, Some(0));
        assert_eq!(hist.positive_span[0].length, Some(0));
    }

    #[test]
    fn test_render_protobuf_native_histogram_populated() {
        let config = crate::native_histogram::NativeHistogramConfig::new(2.0, 160, 0.001).unwrap();
        let mut dist = crate::distribution::Distribution::new_native_histogram(config);
        dist.record_samples(&[
            (1.0, quanta::Instant::now()),
            (2.0, quanta::Instant::now()),
            (-1.0, quanta::Instant::now()),
        ]);

        let snapshot = single_dist("native_pop", dist);
        let (mut wr, rd) = new_description_handles();
        wr.publish();

        let data = render_protobuf(crate::render::render_snapshot_and_descriptions(snapshot, rd, None));
        let hist = decode_one(&data).metric[0].histogram.as_ref().unwrap().clone();

        assert_eq!(hist.sample_count, Some(3));
        assert!(!hist.positive_span.is_empty(), "positive observations -> positive spans");
        assert!(!hist.negative_span.is_empty(), "negative observation -> negative spans");
        assert!(!hist.positive_delta.is_empty());
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

        let rendered_metrics =
            crate::render::render_snapshot_and_descriptions(snapshot, descriptions_rd, None);
        let protobuf_data = render_protobuf(rendered_metrics);

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

    // Deterministic snapshot for the golden test: exactly one counter, one
    // gauge, and one native histogram (one series each). No second distribution
    // family, no summary -> fully stable byte output.
    fn golden_snapshot() -> Snapshot {
        let mk = |name: &str| {
            LabelSet::from_key_and_global(
                &metrics::Key::from_parts(String::from(name), vec![metrics::Label::new("k", "v")]),
                &IndexMap::new(),
            )
        };

        let mut counters = HashMap::new();
        let mut c = HashMap::new();
        c.insert(mk("requests"), 42u64);
        counters.insert("requests".to_string(), c);

        let mut gauges = HashMap::new();
        let mut g = HashMap::new();
        g.insert(mk("temp"), 0.5f64);
        gauges.insert("temp".to_string(), g);

        let now = quanta::Instant::now();
        let config = crate::native_histogram::NativeHistogramConfig::new(2.0, 160, 0.001).unwrap();
        let mut native = crate::distribution::Distribution::new_native_histogram(config);
        native.record_samples(&[(1.0, now), (4.0, now)]);

        let mut distributions = HashMap::new();
        let mut native_map = IndexMap::new();
        native_map.insert(mk("native"), native);
        distributions.insert("native".to_string(), native_map);

        Snapshot { counters, gauges, distributions }
    }

    #[test]
    fn protobuf_output_matches_golden() {
        let snapshot = golden_snapshot();
        let (mut wr, rd) = new_description_handles();
        wr.publish();

        let data = render_protobuf(crate::render::render_snapshot_and_descriptions(
            snapshot,
            rd,
            Some("total"),
        ));

        let golden = include_bytes!("../tests/fixtures/pr686_golden_protobuf.bin");
        assert_eq!(
            data.as_slice(),
            golden.as_slice(),
            "protobuf output changed vs. the bytes captured from main; \
             a material output change was introduced",
        );
    }
}
