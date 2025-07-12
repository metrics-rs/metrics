use metrics::{
    counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram, Recorder, Unit,
};
use metrics_exporter_opentelemetry::OpenTelemetryRecorder;
use opentelemetry::metrics::MeterProvider;
use opentelemetry::{Key, Value};
use opentelemetry_sdk::metrics::data::{AggregatedMetrics, MetricData};
use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};
use std::time::Duration;

#[test]
fn test_counter_increments_correctly() {
    // Given: OpenTelemetry recorder with in-memory exporter
    let exporter = InMemoryMetricExporter::default();
    let reader =
        PeriodicReader::builder(exporter.clone()).with_interval(Duration::from_millis(100)).build();
    let provider = SdkMeterProvider::builder().with_reader(reader).build();
    let meter = provider.meter("test_meter");
    let recorder = OpenTelemetryRecorder::new(meter);
    let _ = metrics::set_global_recorder(recorder);

    // When: Counter is incremented with different labels
    describe_counter!("requests_total", Unit::Count, "Total number of requests");
    counter!("requests_total", "method" => "GET", "status" => "200").increment(1);
    counter!("requests_total", "method" => "POST", "status" => "201").increment(2);
    provider.force_flush().unwrap();

    // Then: Counter values are recorded correctly
    let metrics = exporter.get_finished_metrics().unwrap();
    let requests_metric = metrics
        .last()
        .unwrap()
        .scope_metrics()
        .flat_map(|sm| sm.metrics())
        .find(|m| m.name() == "requests_total")
        .expect("requests_total metric should exist");

    let AggregatedMetrics::U64(metric_data) = requests_metric.data() else {
        panic!("Counter should be U64");
    };
    let MetricData::Sum(sum) = metric_data else {
        panic!("Counter should be Sum");
    };

    let data_points: Vec<_> = sum.data_points().collect();
    assert_eq!(data_points.len(), 2, "Should have 2 data points for different label combinations");

    let get_point = data_points
        .iter()
        .find(|dp| {
            dp.attributes().any(|a| a.key == Key::from("method") && a.value == Value::from("GET"))
        })
        .expect("Should have GET data point");
    assert_eq!(get_point.value(), 1, "GET counter should be 1");

    let post_point = data_points
        .iter()
        .find(|dp| {
            dp.attributes().any(|a| a.key == Key::from("method") && a.value == Value::from("POST"))
        })
        .expect("Should have POST data point");
    assert_eq!(post_point.value(), 2, "POST counter should be 2");
}

#[test]
fn test_counter_accumulates_increments() {
    // Given: OpenTelemetry recorder with counter already incremented
    let exporter = InMemoryMetricExporter::default();
    let reader =
        PeriodicReader::builder(exporter.clone()).with_interval(Duration::from_millis(100)).build();
    let provider = SdkMeterProvider::builder().with_reader(reader).build();
    let meter = provider.meter("test_meter");
    let recorder = OpenTelemetryRecorder::new(meter);
    let _ = metrics::set_global_recorder(recorder);

    counter!("events_total").increment(5);
    provider.force_flush().unwrap();

    // Then: First flush should have counter value of 5
    let metrics = exporter.get_finished_metrics().unwrap();
    assert!(!metrics.is_empty(), "Should have metrics after first flush");
    let first_metric = metrics[0]
        .scope_metrics()
        .flat_map(|sm| sm.metrics())
        .find(|m| m.name() == "events_total")
        .expect("events_total metric should exist in first flush");

    let AggregatedMetrics::U64(first_data) = first_metric.data() else {
        panic!("Counter should be U64");
    };
    let MetricData::Sum(first_sum) = first_data else {
        panic!("Counter should be Sum");
    };

    let first_point = first_sum.data_points().next().expect("Should have data point");
    assert_eq!(first_point.value(), 5, "First flush should have counter value of 5");

    // When: Same counter is incremented again
    counter!("events_total").increment(3);
    provider.force_flush().unwrap();

    // Then: Counter value should accumulate in the last metric
    let metrics = exporter.get_finished_metrics().unwrap();
    let events_metric = metrics
        .last()
        .unwrap()
        .scope_metrics()
        .flat_map(|sm| sm.metrics())
        .find(|m| m.name() == "events_total")
        .expect("events_total metric should exist");

    let AggregatedMetrics::U64(metric_data) = events_metric.data() else {
        panic!("Counter should be U64");
    };
    let MetricData::Sum(sum) = metric_data else {
        panic!("Counter should be Sum");
    };

    let point = sum.data_points().next().expect("Should have data point");
    assert_eq!(point.value(), 8, "Counter should accumulate to 8");
}

#[test]
fn test_gauge_sets_value_correctly() {
    // Given: OpenTelemetry recorder with in-memory exporter
    let exporter = InMemoryMetricExporter::default();
    let reader =
        PeriodicReader::builder(exporter.clone()).with_interval(Duration::from_millis(100)).build();
    let provider = SdkMeterProvider::builder().with_reader(reader).build();
    let meter = provider.meter("test_meter");
    let recorder = OpenTelemetryRecorder::new(meter);
    let _ = metrics::set_global_recorder(recorder);

    // When: Gauge values are set with different labels
    describe_gauge!("cpu_usage", Unit::Percent, "Current CPU usage");
    gauge!("cpu_usage", "core" => "0").set(45.5);
    gauge!("cpu_usage", "core" => "1").set(62.3);
    provider.force_flush().unwrap();

    // Then: Gauge values are recorded correctly
    let metrics = exporter.get_finished_metrics().unwrap();
    let cpu_metric = metrics
        .last()
        .unwrap()
        .scope_metrics()
        .flat_map(|sm| sm.metrics())
        .find(|m| m.name() == "cpu_usage")
        .expect("cpu_usage metric should exist");

    let AggregatedMetrics::F64(metric_data) = cpu_metric.data() else {
        panic!("Gauge should be F64");
    };
    let MetricData::Gauge(gauge_data) = metric_data else {
        panic!("Gauge should be Gauge type");
    };

    let data_points: Vec<_> = gauge_data.data_points().collect();
    assert_eq!(data_points.len(), 2, "Should have 2 data points for different cores");

    let core0_point = data_points
        .iter()
        .find(|dp| {
            dp.attributes().any(|a| a.key == Key::from("core") && a.value == Value::from("0"))
        })
        .expect("Should have core 0 data point");
    assert_eq!(core0_point.value(), 45.5, "Core 0 usage should be 45.5");

    let core1_point = data_points
        .iter()
        .find(|dp| {
            dp.attributes().any(|a| a.key == Key::from("core") && a.value == Value::from("1"))
        })
        .expect("Should have core 1 data point");
    assert_eq!(core1_point.value(), 62.3, "Core 1 usage should be 62.3");
}

#[test]
fn test_gauge_updates_value() {
    // Given: OpenTelemetry recorder with gauge already set
    let exporter = InMemoryMetricExporter::default();
    let reader =
        PeriodicReader::builder(exporter.clone()).with_interval(Duration::from_millis(100)).build();
    let provider = SdkMeterProvider::builder().with_reader(reader).build();
    let meter = provider.meter("test_meter");
    let recorder = OpenTelemetryRecorder::new(meter);
    let _ = metrics::set_global_recorder(recorder);

    gauge!("memory_usage").set(1024.0);
    provider.force_flush().unwrap();

    // When: Gauge value is updated
    gauge!("memory_usage").set(2048.0);
    provider.force_flush().unwrap();

    // Then: Latest gauge value should be recorded
    let metrics = exporter.get_finished_metrics().unwrap();
    let memory_metric = metrics
        .last()
        .unwrap()
        .scope_metrics()
        .flat_map(|sm| sm.metrics())
        .find(|m| m.name() == "memory_usage")
        .expect("memory_usage metric should exist");

    let AggregatedMetrics::F64(metric_data) = memory_metric.data() else {
        panic!("Gauge should be F64");
    };
    let MetricData::Gauge(gauge_data) = metric_data else {
        panic!("Gauge should be Gauge type");
    };

    let point = gauge_data.data_points().next().expect("Should have data point");
    assert_eq!(point.value(), 2048.0, "Gauge should have latest value 2048.0");
}

#[test]
fn test_histogram_records_values() {
    // Given: OpenTelemetry recorder with in-memory exporter
    let exporter = InMemoryMetricExporter::default();
    let reader =
        PeriodicReader::builder(exporter.clone()).with_interval(Duration::from_millis(100)).build();
    let provider = SdkMeterProvider::builder().with_reader(reader).build();
    let meter = provider.meter("test_meter");
    let recorder = OpenTelemetryRecorder::new(meter);
    let _ = metrics::set_global_recorder(recorder);

    // When: Histogram values are recorded
    describe_histogram!("response_time", Unit::Seconds, "Response time distribution");
    histogram!("response_time", "endpoint" => "/api/users").record(0.123);
    histogram!("response_time", "endpoint" => "/api/users").record(0.456);
    histogram!("response_time", "endpoint" => "/api/posts").record(0.789);
    provider.force_flush().unwrap();

    // Then: Histogram values are recorded correctly
    let metrics = exporter.get_finished_metrics().unwrap();
    let response_metric = metrics
        .last()
        .unwrap()
        .scope_metrics()
        .flat_map(|sm| sm.metrics())
        .find(|m| m.name() == "response_time")
        .expect("response_time metric should exist");

    let AggregatedMetrics::F64(metric_data) = response_metric.data() else {
        panic!("Histogram should be F64");
    };
    let MetricData::Histogram(hist_data) = metric_data else {
        panic!("Should be Histogram type");
    };

    let data_points: Vec<_> = hist_data.data_points().collect();
    assert_eq!(data_points.len(), 2, "Should have 2 data points for different endpoints");

    let users_point = data_points
        .iter()
        .find(|dp| {
            dp.attributes()
                .any(|a| a.key == Key::from("endpoint") && a.value == Value::from("/api/users"))
        })
        .expect("Should have /api/users data point");
    assert_eq!(users_point.count(), 2, "/api/users should have 2 recordings");
    assert_eq!(users_point.sum(), 0.123 + 0.456, "Sum should be correct");

    let posts_point = data_points
        .iter()
        .find(|dp| {
            dp.attributes()
                .any(|a| a.key == Key::from("endpoint") && a.value == Value::from("/api/posts"))
        })
        .expect("Should have /api/posts data point");
    assert_eq!(posts_point.count(), 1, "/api/posts should have 1 recording");
    assert_eq!(posts_point.sum(), 0.789, "Sum should be 0.789");
}

#[test]
fn test_metrics_without_descriptions() {
    // Given: OpenTelemetry recorder with in-memory exporter
    let exporter = InMemoryMetricExporter::default();
    let reader =
        PeriodicReader::builder(exporter.clone()).with_interval(Duration::from_millis(100)).build();
    let provider = SdkMeterProvider::builder().with_reader(reader).build();
    let meter = provider.meter("test_meter");
    let recorder = OpenTelemetryRecorder::new(meter);
    let _ = metrics::set_global_recorder(recorder);

    // When: Metrics are used without descriptions
    counter!("events").increment(10);
    gauge!("temperature").set(23.5);
    histogram!("duration").record(1.5);
    provider.force_flush().unwrap();

    // Then: Metrics should still be recorded
    let metrics = exporter.get_finished_metrics().unwrap();
    let scope_metrics: Vec<_> = metrics.last().unwrap().scope_metrics().collect();
    assert!(!scope_metrics.is_empty(), "Should have scope metrics");

    let all_metrics: Vec<_> = scope_metrics.iter().flat_map(|sm| sm.metrics()).collect();

    assert!(all_metrics.iter().any(|m| m.name() == "events"), "Should have events counter");
    assert!(all_metrics.iter().any(|m| m.name() == "temperature"), "Should have temperature gauge");
    assert!(all_metrics.iter().any(|m| m.name() == "duration"), "Should have duration histogram");
}

#[test]
fn test_metric_descriptions_are_stored() {
    // Given: OpenTelemetry recorder
    let exporter = InMemoryMetricExporter::default();
    let reader =
        PeriodicReader::builder(exporter.clone()).with_interval(Duration::from_millis(100)).build();
    let provider = SdkMeterProvider::builder().with_reader(reader).build();
    let meter = provider.meter("test_meter");
    let recorder = OpenTelemetryRecorder::new(meter);

    // When: Metrics are described directly on the recorder
    recorder.describe_counter(
        "test.counter".into(),
        Some(Unit::Count),
        "Test counter description".into(),
    );
    recorder.describe_gauge(
        "test.gauge".into(),
        Some(Unit::Bytes),
        "Test gauge description".into(),
    );
    recorder.describe_histogram(
        "test.histogram".into(),
        Some(Unit::Milliseconds),
        "Test histogram description".into(),
    );

    // And: Metrics are registered and used
    let key = metrics::Key::from_name("test.counter");
    let metadata = metrics::Metadata::new("test", metrics::Level::INFO, Some("test.counter"));
    let counter = recorder.register_counter(&key, &metadata);
    counter.increment(1);

    let key = metrics::Key::from_name("test.gauge");
    let gauge = recorder.register_gauge(&key, &metadata);
    gauge.set(42.0);

    let key = metrics::Key::from_name("test.histogram");
    let histogram = recorder.register_histogram(&key, &metadata);
    histogram.record(0.5);

    provider.force_flush().unwrap();

    // Then: Metrics should have correct descriptions and units
    let metrics = exporter.get_finished_metrics().unwrap();
    let all_metrics: Vec<_> =
        metrics.last().unwrap().scope_metrics().flat_map(|sm| sm.metrics()).collect();

    assert_eq!(all_metrics.len(), 3, "Should have all 3 metrics registered");

    // Check counter description and unit
    let counter_metric = all_metrics
        .iter()
        .find(|m| m.name() == "test.counter")
        .expect("Should have test.counter metric");
    assert_eq!(
        counter_metric.description(),
        "Test counter description",
        "Counter should have correct description"
    );
    assert_eq!(counter_metric.unit(), "", "Counter should have unit '' for Count");

    // Check gauge description and unit
    let gauge_metric = all_metrics
        .iter()
        .find(|m| m.name() == "test.gauge")
        .expect("Should have test.gauge metric");
    assert_eq!(
        gauge_metric.description(),
        "Test gauge description",
        "Gauge should have correct description"
    );
    assert_eq!(gauge_metric.unit(), "B", "Gauge should have unit 'B' for Bytes");

    // Check histogram description and unit
    let histogram_metric = all_metrics
        .iter()
        .find(|m| m.name() == "test.histogram")
        .expect("Should have test.histogram metric");
    assert_eq!(
        histogram_metric.description(),
        "Test histogram description",
        "Histogram should have correct description"
    );
    assert_eq!(histogram_metric.unit(), "ms", "Histogram should have unit 'ms' for Milliseconds");
}

#[test]
fn test_metrics_with_multiple_labels() {
    // Given: OpenTelemetry recorder
    let exporter = InMemoryMetricExporter::default();
    let reader =
        PeriodicReader::builder(exporter.clone()).with_interval(Duration::from_millis(100)).build();
    let provider = SdkMeterProvider::builder().with_reader(reader).build();
    let meter = provider.meter("test_meter");
    let recorder = OpenTelemetryRecorder::new(meter);

    // When: Metrics are registered with multiple labels
    let key = metrics::Key::from_parts(
        "http_requests",
        vec![
            metrics::Label::new("method", "GET"),
            metrics::Label::new("status", "200"),
            metrics::Label::new("path", "/api/v1/users"),
        ],
    );
    let metadata = metrics::Metadata::new("test", metrics::Level::INFO, Some("http_requests"));
    let counter = recorder.register_counter(&key, &metadata);
    counter.increment(5);

    provider.force_flush().unwrap();

    // Then: All labels should be recorded as attributes
    let metrics = exporter.get_finished_metrics().unwrap();
    let http_metric = metrics
        .last()
        .unwrap()
        .scope_metrics()
        .flat_map(|sm| sm.metrics())
        .find(|m| m.name() == "http_requests")
        .expect("http_requests metric should exist");

    let AggregatedMetrics::U64(metric_data) = http_metric.data() else {
        panic!("Counter should be U64");
    };
    let MetricData::Sum(sum) = metric_data else {
        panic!("Counter should be Sum");
    };

    let point = sum.data_points().next().expect("Should have data point");
    let attrs: Vec<_> = point.attributes().collect();

    assert_eq!(attrs.len(), 3, "Should have 3 attributes");
    assert!(attrs.iter().any(|a| a.key == Key::from("method") && a.value == Value::from("GET")));
    assert!(attrs.iter().any(|a| a.key == Key::from("status") && a.value == Value::from("200")));
    assert!(attrs
        .iter()
        .any(|a| a.key == Key::from("path") && a.value == Value::from("/api/v1/users")));
    assert_eq!(point.value(), 5, "Counter value should be 5");
}
