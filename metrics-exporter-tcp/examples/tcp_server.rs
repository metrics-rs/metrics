use std::thread;
use std::time::Duration;

use metrics::{counter, describe_histogram, gauge, histogram, Unit};
use metrics_exporter_tcp::TcpBuilder;

use quanta::Clock;
use rand::{thread_rng, Rng};

fn main() {
    tracing_subscriber::fmt::init();

    let builder = TcpBuilder::new();
    builder.install().expect("failed to install TCP recorder");

    let clock = Clock::new();
    let mut last = None;

    describe_histogram!(
        "tcp_server_loop_delta_secs",
        Unit::Seconds,
        "amount of time spent in the core server loop["
    );

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

        let sleep_time = thread_rng().gen_range(250..750);

        thread::sleep(Duration::from_millis(sleep_time));
    }
}
