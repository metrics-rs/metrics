# metrics-exporter-opentelemetry

[![docs-badge][docs-badge]][docs] [![crates-badge][crates-badge]][crates] [![license-badge][license-badge]][license]

[docs-badge]: https://docs.rs/metrics-exporter-opentelemetry/badge.svg
[docs]: https://docs.rs/metrics-exporter-opentelemetry
[crates-badge]: https://img.shields.io/crates/v/metrics-exporter-opentelemetry.svg
[crates]: https://crates.io/crates/metrics-exporter-opentelemetry
[license-badge]: https://img.shields.io/crates/l/metrics-exporter-opentelemetry.svg
[license]: #license

A [`metrics`]-compatible exporter for sending metrics to OpenTelemetry collectors.

[`metrics`]: https://docs.rs/metrics/

## Overview

A [`metrics`]-compatible exporter for OpenTelemetry collectors and OTLP endpoints.

## Features

- Counters, gauges, and histograms
- Custom histogram bucket boundaries
- Metric descriptions and units
- Lock-free concurrent data structures
- Works with any OpenTelemetry [`Meter`]

[`Meter`]: https://docs.rs/opentelemetry/latest/opentelemetry/metrics/trait.Meter.html

## Quick Start

Add this to your `Cargo.toml`:

```toml
[dependencies]
metrics = "0.24"
metrics-exporter-opentelemetry = "0.1"
opentelemetry = "0.30"
opentelemetry_sdk = "0.30"
```

Basic usage:

```rust
use metrics_exporter_opentelemetry::OpenTelemetryRecorder;
use opentelemetry::metrics::MeterProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;

// Create an OpenTelemetry meter
let provider = SdkMeterProvider::default();
let meter = provider.meter("my_application");

// Create and install the recorder
let recorder = OpenTelemetryRecorder::new(meter);
metrics::set_global_recorder(recorder).expect("failed to install recorder");

// Use metrics as normal
metrics::counter!("requests_total", "method" => "GET").increment(1);
metrics::gauge!("cpu_usage", "core" => "0").set(45.2);
metrics::histogram!("response_time", "endpoint" => "/api/users").record(0.123);
```

## Custom Histogram Boundaries

```rust
let recorder = OpenTelemetryRecorder::new(meter);

recorder.set_histogram_bounds(
    &metrics::KeyName::from("response_time"),
    vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
);

metrics::set_global_recorder(recorder).expect("failed to install recorder");
```

## Metric Descriptions and Units

You can provide descriptions and units for your metrics using the `describe_*` macros. 

CAUTION: These macros must be called before the metrics are recorded. The instruments created before calling these macros will not have descriptions or units.

```rust
use metrics::{describe_counter, describe_histogram, Unit};

describe_counter!("requests_total", Unit::Count, "Total HTTP requests");
describe_histogram!("response_time", Unit::Seconds, "Response time distribution");

metrics::counter!("requests_total").increment(1);
metrics::histogram!("response_time").record(0.045);
```

## Compatibility

### Metric Type Mapping

| `metrics` Type | OpenTelemetry Instrument |
|----------------|-------------------------|
| `Counter` | `ObservableCounter` (u64) |
| `Gauge` | `ObservableGauge` (f64) |
| `Histogram` | `Histogram` (f64) |
