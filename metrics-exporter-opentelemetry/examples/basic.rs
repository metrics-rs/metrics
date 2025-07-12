use metrics::{counter, describe_counter, describe_histogram, gauge, histogram, Unit};
use metrics_exporter_opentelemetry::OpenTelemetryRecorder;
use opentelemetry::metrics::MeterProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use std::thread;
use std::time::Duration;

fn main() {
    // Create OpenTelemetry provider and meter
    let provider = SdkMeterProvider::default();
    let meter = provider.meter("example_app");

    // Create and install the OpenTelemetry recorder
    let recorder = OpenTelemetryRecorder::new(meter);
    metrics::set_global_recorder(recorder).expect("failed to install recorder");

    // Register metrics with descriptions and units
    describe_counter!("requests_total", Unit::Count, "Total HTTP requests");
    describe_histogram!("response_time", Unit::Seconds, "Response time distribution");

    // Loop and record metrics
    for i in 0..10 {
        counter!("requests_total", "method" => "GET", "status" => "200").increment(1);
        gauge!("cpu_usage").set(45.0 + (i as f64 * 2.0));
        histogram!("response_time").record(0.1 + (i as f64 * 0.01));

        println!("Recorded metrics iteration {}", i + 1);
        thread::sleep(Duration::from_millis(500));
    }

    println!("Example completed");
}
