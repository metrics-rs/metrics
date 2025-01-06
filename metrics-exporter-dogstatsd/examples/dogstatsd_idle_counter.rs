use std::time::{Duration, Instant};

use metrics::counter;
use metrics_exporter_dogstatsd::DogStatsDBuilder;

fn main() {
    tracing_subscriber::fmt::init();

    DogStatsDBuilder::default()
        .with_remote_address("localhost:9125")
        .expect("failed to parse remote address")
        .with_telemetry(false)
        .install()
        .expect("failed to install DogStatsD recorder");

    counter!("idle_metric").increment(1);

    // Loop over and over, incrementing our counter every 10 seconds or so.
    let mut last_update = Instant::now();
    loop {
        if last_update.elapsed() > Duration::from_secs(10) {
            counter!("idle_metric").increment(1);
            last_update = Instant::now();
        }

        std::thread::sleep(Duration::from_secs(1));
    }
}
