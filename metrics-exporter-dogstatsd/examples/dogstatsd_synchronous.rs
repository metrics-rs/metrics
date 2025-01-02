use metrics::{counter, gauge, histogram};
use metrics_exporter_dogstatsd::DogStatsDBuilder;
use rand::{thread_rng, Rng, SeedableRng as _};
use rand_xoshiro::Xoshiro256StarStar;

fn main() {
    tracing_subscriber::fmt::init();

    DogStatsDBuilder::default()
        .with_remote_address("localhost:9125")
        .expect("failed to parse remote address")
        .install()
        .expect("failed to install DogStatsD recorder");

    counter!("idle_metric").increment(1);
    gauge!("testing").set(42.0);

    let server_loops = counter!("tcp_server_loops", "system" => "foo");
    let server_loops_delta_secs = histogram!("tcp_server_loop_delta_secs", "system" => "foo");

    let mut rng = Xoshiro256StarStar::from_rng(thread_rng()).unwrap();

    // Loop over and over, pretending to do some work.
    loop {
        server_loops.increment(1);
        server_loops_delta_secs.record(rng.gen_range(0.0..1.0));

        let increment_gauge = thread_rng().gen_bool(0.75);
        let gauge = gauge!("lucky_iterations");
        if increment_gauge {
            gauge.increment(1.0);
        } else {
            gauge.decrement(1.0);
        }
    }
}
