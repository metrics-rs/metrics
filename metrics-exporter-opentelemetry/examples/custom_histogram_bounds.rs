use metrics::{describe_histogram, histogram, KeyName, Unit};
use metrics_exporter_opentelemetry::OpenTelemetryRecorder;
use opentelemetry::metrics::MeterProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use std::thread;
use std::time::Duration;

fn main() {
    // Create OpenTelemetry provider and meter
    let provider = SdkMeterProvider::default();
    let meter = provider.meter("histogram_example");

    // Create the recorder
    let recorder = OpenTelemetryRecorder::new(meter);

    // Set custom histogram boundaries BEFORE installing the recorder
    recorder.set_histogram_bounds(
        &KeyName::from("latency"),
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0],
    );

    recorder.set_histogram_bounds(
        &KeyName::from("request_size"),
        vec![1024.0, 4096.0, 16384.0, 65536.0, 262144.0, 1048576.0],
    );

    // Install the recorder
    metrics::set_global_recorder(recorder).expect("failed to install recorder");

    // Describe the histograms
    describe_histogram!("latency", Unit::Seconds, "Request latency");
    describe_histogram!("request_size", Unit::Bytes, "Request size");

    // Record various values to see the custom buckets in action
    let latency_values = [0.002, 0.008, 0.015, 0.035, 0.075, 0.15, 0.3, 0.7, 1.5, 3.0, 7.0];
    let size_values = [512.0, 2048.0, 8192.0, 32768.0, 131072.0, 524288.0, 2097152.0];

    for (i, &latency) in latency_values.iter().enumerate() {
        histogram!("latency", "endpoint" => "/api/users").record(latency);

        if i < size_values.len() {
            histogram!("request_size", "endpoint" => "/api/users").record(size_values[i]);
        }

        println!("Recorded latency: {}s, size: {}B", latency, size_values.get(i).unwrap_or(&0.0));
        thread::sleep(Duration::from_millis(300));
    }

    println!("Custom histogram bounds example completed");
}
