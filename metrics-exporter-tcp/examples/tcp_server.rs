use std::thread;
use std::time::Duration;

use metrics::{
    decrement_gauge, histogram, counter, increment_gauge,
    register_counter, register_gauge, register_histogram, Unit,
};
use metrics_exporter_tcp::TcpBuilder;

use quanta::Clock;
use rand::{thread_rng, Rng};

fn main() {
    tracing_subscriber::fmt::init();

    let builder = TcpBuilder::new();
    builder.install().expect("failed to install TCP recorder");

    let mut clock = Clock::new();
    let mut last = None;

    register_counter!("sent_tb", Unit::Tebibytes);
    register_histogram!("tcp_server_loop_delta_ns", Unit::Nanoseconds);
    register_gauge!("disk_utilization", Unit::Percent);

    loop {
        counter!("sent_tb", thread_rng().gen_range(100000, 101212));//, "system" => "foo");

        if let Some(t) = last {
            let delta: Duration = clock.now() - t;
            histogram!("tcp_server_loop_delta_ns", delta);//, "system" => "foo");
        }

        let increment_gauge = thread_rng().gen_bool(0.75);
        if increment_gauge {
            increment_gauge!("disk_utilization", 7.5);
        } else {
            decrement_gauge!("disk_utilization", 3.15);
        }

        last = Some(clock.now());

        let sleep_time = thread_rng().gen_range(250, 1500);

        thread::sleep(Duration::from_millis(sleep_time));
    }
}
