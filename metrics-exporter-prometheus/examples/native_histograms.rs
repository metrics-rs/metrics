use metrics::histogram;
use metrics_exporter_prometheus::{NativeHistogramConfig, PrometheusBuilder};
use std::thread;
use std::time::Duration;

fn main() {
    // Create a Prometheus builder with native histograms enabled for specific metrics
    let builder = PrometheusBuilder::new()
        .with_http_listener(([127, 0, 0, 1], 9000))
        // Enable native histograms for specific request_duration metrics
        .set_native_histogram_for_metric(
            metrics_exporter_prometheus::Matcher::Prefix("request_duration_api".to_string()),
            NativeHistogramConfig::new(1.1, 160, 1e-9).unwrap(), // Finer granularity
        )
        .set_native_histogram_for_metric(
            metrics_exporter_prometheus::Matcher::Prefix("response_size".to_string()),
            NativeHistogramConfig::new(1.1, 160, 1e-9).unwrap(), // Finer granularity
        );

    // Install the recorder and get a handle
    builder.install().expect("failed to install recorder");

    // Simulate some metric recording in a loop
    println!("Recording metrics... Check http://127.0.0.1:9000/metrics");
    println!("Native histograms will only be visible in protobuf format.");
    println!(
        "Try: curl -H 'Accept: application/vnd.google.protobuf' http://127.0.0.1:9000/metrics"
    );

    for i in 0..1000 {
        // Record to native histogram (request_duration_api)
        let duration = (i as f64 / 10.0).sin().abs() * 5.0 + 0.1;
        histogram!("request_duration_api").record(duration);

        // Record to regular histogram (response_size)
        let size = 1000.0 + (i as f64).cos() * 500.0;
        histogram!("response_size").record(size);

        if i % 100 == 0 {
            println!("Recorded {} samples", i + 1);
        }

        thread::sleep(Duration::from_millis(10));
    }

    println!("Metrics server will continue running. Access http://127.0.0.1:9000/metrics");
    println!("Press Ctrl+C to exit");

    // Keep the server running
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}
