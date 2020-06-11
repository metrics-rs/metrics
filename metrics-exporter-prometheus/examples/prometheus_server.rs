use std::thread;
use std::time::Duration;

use metrics::{histogram, increment};
use metrics_exporter_prometheus::PrometheusBuilder;

use quanta::Clock;

fn main() {
    tracing_subscriber::fmt::init();

    let builder = PrometheusBuilder::new();
    builder
        .install()
        .expect("failed to install Prometheus recorder");

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
