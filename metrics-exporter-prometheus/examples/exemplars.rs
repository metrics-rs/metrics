use std::thread;
use std::time::Duration;

use metrics::{
    decrement_gauge, describe_counter, describe_histogram, gauge, histogram, increment_counter,
    increment_gauge, Key, Label,
};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusRecorder};
use metrics_util::MetricKindMask;

use quanta::Clock;
use rand::{thread_rng, Rng};

fn main() {
    tracing_subscriber::fmt::init();

    let builder = PrometheusBuilder::new();
    builder
        .idle_timeout(
            MetricKindMask::COUNTER | MetricKindMask::HISTOGRAM,
            Some(Duration::from_secs(10)),
        )
        .set_buckets(&[0.01, 0.1, 0.2, 0.5, 1., 2.])
        .unwrap()
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

    increment_counter!("idle_metric");
    gauge!("testing", 42.0);

    // Loop over and over, pretending to do some work.
    loop {
        let with_exemplar = thread_rng().gen_bool(0.75);
        if with_exemplar {
            let labels = vec![Label::new("system", "foo")];
            let metric_key = Key::from_parts("tcp_server_loops", labels);
            let recorder = metrics::recorder()
                .downcast_ref::<PrometheusRecorder>()
                .expect("Couldn't downcast to prometheus recorder");
            let handler = recorder.register_counter_with_exemplar(&metric_key);
            handler.increment_with_exemplar(1, vec![Label::new("trace_id", "xyasfj234234")]);
        } else {
            increment_counter!("tcp_server_loops", "system" => "foo");
        }

        if let Some(t) = last {
            let delta: Duration = clock.now() - t;
            if with_exemplar {
                let labels = vec![Label::new("system", "foo")];
                let metric_key = Key::from_parts("tcp_server_loops", labels);
                let recorder = metrics::recorder()
                    .downcast_ref::<PrometheusRecorder>()
                    .expect("Couldn't downcast to prometheus recorder");
                let distribution = recorder.get_distribution(metric_key.name());
                let handler = recorder.register_histogram_with_exemplar(&metric_key);
                handler.record_with_exemplar(
                    distribution,
                    metrics::__into_f64(delta),
                    vec![Label::new("trace_id", "xyasfj234234")],
                );
            } else {
                histogram!("tcp_server_loop_delta_secs", delta, "system" => "foo");
            }
        }

        let increment_gauge = thread_rng().gen_bool(0.75);
        if increment_gauge {
            increment_gauge!("lucky_iterations", 1.0);
        } else {
            decrement_gauge!("lucky_iterations", 1.0);
        }

        last = Some(clock.now());

        let render = thread_rng().gen_bool(0.1);
        if render {
            let recorder = metrics::recorder()
                .downcast_ref::<PrometheusRecorder>()
                .expect("Couldn't downcast to prometheus recorder");
            println!("{}", recorder.handle().render());
        }

        thread::sleep(Duration::from_millis(750));
    }
}
