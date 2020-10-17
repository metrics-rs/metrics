use std::thread;
use std::time::Duration;

use metrics::{histogram, increment, register_histogram, Unit};
use metrics_exporter_tcp::TcpBuilder;

use quanta::Clock;

fn main() {
    tracing_subscriber::fmt::init();

    let builder = TcpBuilder::new();
    builder.install().expect("failed to install TCP recorder");

    let mut clock = Clock::new();
    let mut last = None;

    register_histogram!("tcp_server_loop_delta_ns", Unit::Nanoseconds);

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
