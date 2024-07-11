use std::thread;
use std::time::Duration;

use metrics::{counter, describe_counter, describe_histogram, gauge, histogram};
use metrics_exporter_prometheus::PrometheusBuilder;
use metrics_util::MetricKindMask;

use quanta::Clock;
use rand::{thread_rng, Rng};

fn main() {
    tracing_subscriber::fmt::init();

    let builder = PrometheusBuilder::new().with_http_uds_listener("/tmp/metrics.sock");
    builder
        .idle_timeout(
            MetricKindMask::COUNTER | MetricKindMask::HISTOGRAM,
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
    describe_counter!("tcp_server_loops", "The iterations of the TCP server event loop so far.");
    describe_histogram!(
        "tcp_server_loop_delta_secs",
        "The time taken for iterations of the TCP server event loop."
    );

    let clock = Clock::new();
    let mut last = None;

    counter!("idle_metric").increment(1);
    gauge!("testing").set(42.0);

    // Loop over and over, pretending to do some work.
    loop {
        counter!("tcp_server_loops", "system" => "foo").increment(1);

        if let Some(t) = last {
            let delta: Duration = clock.now() - t;
            histogram!("tcp_server_loop_delta_secs", "system" => "foo").record(delta);
        }

        let increment_gauge = thread_rng().gen_bool(0.75);
        let gauge = gauge!("lucky_iterations");
        if increment_gauge {
            gauge.increment(1.0);
        } else {
            gauge.decrement(1.0);
        }

        last = Some(clock.now());

        thread::sleep(Duration::from_millis(750));
    }
}
