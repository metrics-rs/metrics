/// Make sure to run this example with `--features push-gateway` to properly enable push gateway support.
#[allow(unused_imports)]
use std::thread;
use std::time::Duration;

#[allow(unused_imports)]
use metrics::{
    decrement_gauge, gauge, histogram, increment_counter, increment_gauge, register_counter,
    register_histogram,
};
#[allow(unused_imports)]
use metrics_exporter_prometheus::PrometheusBuilder;
#[allow(unused_imports)]
use metrics_util::MetricKindMask;

use quanta::Clock;
use rand::{thread_rng, Rng};

fn main() {
    tracing_subscriber::fmt::init();

    #[allow(unused_mut)]
    let mut builder = PrometheusBuilder::new()
        .disable_http_listener()
        .idle_timeout(
            MetricKindMask::COUNTER | MetricKindMask::HISTOGRAM,
            Some(Duration::from_secs(10)),
        );

    #[cfg(feature = "push-gateway")]
    {
        builder = builder.push_gateway_config(
            "http://127.0.0.1:9091/metrics/job/example",
            Duration::from_secs(10),
        );
    }

    builder
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

    let clock = Clock::new();
    let mut last = None;

    increment_counter!("idle_metric");
    gauge!("testing", 42.0);

    // Loop over and over, pretending to do some work.
    loop {
        increment_counter!("tcp_server_loops", "system" => "foo");

        if let Some(t) = last {
            let delta: Duration = clock.now() - t;
            histogram!("tcp_server_loop_delta_ns", delta, "system" => "foo");
        }

        let increment_gauge = thread_rng().gen_bool(0.75);
        if increment_gauge {
            increment_gauge!("lucky_iterations", 1.0);
        } else {
            decrement_gauge!("lucky_iterations", 1.0);
        }

        last = Some(clock.now());

        thread::sleep(Duration::from_millis(750));
    }
}
