use std::thread;
use std::time::Duration;

use metrics::{register_counter, register_histogram, histogram, increment};
use metrics_exporter_prometheus::PrometheusBuilder;

use quanta::Clock;

fn main() {
    tracing_subscriber::fmt::init();

    let builder = PrometheusBuilder::new();
    builder
        .install()
        .expect("failed to install Prometheus recorder");

    register_counter!("tcp_server_loops", "The iterations of the TCP server event loop so far.");
    register_histogram!("tcp_server_loop_delta_ns", "The time taken for iterations of the TCP server event loop.");

    let clock = Clock::new();
    let mut last = None;

    loop {
        increment!("tcp_server_loops", "system" => "foo");

        if let Some(t) = last {
            let delta: Duration = clock.now() - t;
            histogram!("tcp_server_loop_delta_ns", delta, "system" => "foo");
        }

        last = Some(clock.now());

        thread::sleep(Duration::from_millis(750));
    }
}
