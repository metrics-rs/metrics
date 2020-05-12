use std::thread;
use std::time::{Duration, Instant};

use metrics::{histogram, increment};
use metrics_tcp::TcpBuilder;

use quanta::Clock;

fn main() {
    let builder = TcpBuilder::new();
    builder.install().expect("failed to install TCP recorder");

    let clock = Clock::new();
    let mut last = None;

    loop {
        increment!("tcp_server_loops", "system" => "foo");

        if let Some(t) = last {
            let delta: Duration = clock.now() - t;
            histogram!("tcp_server_loop_delta_ns", delta.as_secs_f64(), "system" => "foo");
        }

        last = Some(clock.now());

        thread::sleep(Duration::from_millis(750));
    }
}
