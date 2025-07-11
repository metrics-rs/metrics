# metrics-exporter-opentelemetry

[![Documentation](https://docs.rs/metrics-exporter-opentelemetry/badge.svg)](https://docs.rs/metrics-exporter-opentelemetry)

A [`metrics`][metrics] exporter for [OpenTelemetry].

## Features

- Export metrics to OpenTelemetry collectors using OTLP
- Support for counters, gauges, and histograms
- Integration with the OpenTelemetry SDK
- Configurable export intervals and endpoints

## Usage

```rust
use metrics::{counter, gauge, histogram};
use metrics_exporter_opentelemetry::OpenTelemetryBuilder;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    metrics::{PeriodicReader, SdkMeterProvider},
    runtime,
};

// Configure OpenTelemetry exporter
let exporter = opentelemetry_otlp::MetricExporter::builder()
    .with_tonic()
    .with_endpoint("http://localhost:4317")
    .build()?;

// Create a periodic reader
let reader = PeriodicReader::builder(exporter, runtime::Tokio)
    .with_interval(Duration::from_secs(10))
    .build();

// Build the meter provider
let provider = SdkMeterProvider::builder()
    .with_reader(reader)
    .build();

// Install the metrics exporter
OpenTelemetryBuilder::new()
    .with_meter_provider(provider)
    .install()?;

// Now you can use the metrics macros
counter!("requests_total").increment(1);
gauge!("cpu_usage").set(0.75);
histogram!("request_duration").record(0.234);
```

## Examples

- [`opentelemetry_push`](examples/opentelemetry_push.rs): Demonstrates exporting metrics to an OpenTelemetry collector
- [`opentelemetry_stdout`](examples/opentelemetry_stdout.rs): Shows metrics export to stdout for debugging

## License

This project is licensed under the MIT license.

[metrics]: https://github.com/metrics-rs/metrics
[OpenTelemetry]: https://opentelemetry.io/