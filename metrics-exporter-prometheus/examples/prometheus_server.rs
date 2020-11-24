use std::thread;
use std::time::Duration;

use metrics::{gauge, histogram, increment, register_counter, register_histogram};
use metrics_exporter_prometheus::PrometheusBuilder;
use metrics_util::MetricKind;

use quanta::Clock;

fn main() {
    tracing_subscriber::fmt::init();

    let builder = PrometheusBuilder::new();
    builder
        .idle_timeout(
            MetricKind::COUNTER | MetricKind::HISTOGRAM,
            Some(Duration::from_secs(10)),
        )
        .install()
        .expect("failed to install Prometheus recorder");

    // We register these metrics, which gives us a chance to specify a description for them.  The
    // Prometheus exporter records this description and adds it as HELP text when the endpoint is
    // scraped.
    //
    // Registering metrics ahead of using them is not required, but is the only way to specify the
    // description of a metric.
    register_counter!(
        "tcp_server_loops",
        "The iterations of the TCP server event loop so far."
    );
    register_histogram!(
        "tcp_server_loop_delta_ns",
        "The time taken for iterations of the TCP server event loop."
    );

    let mut clock = Clock::new();
    let mut last = None;

    increment!("idle_metric");
    gauge!("testing", 42.0);

    // Loop over and over, pretending to do some work.
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
