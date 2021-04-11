//! This example is part unit test and part demonstration.
//!
//! We show all of the registration macros, as well as all of the "emission" macros, the ones you
//! would actually call to update a metric.
//!
//! We demonstrate the various permutations of values that can be passed in the macro calls, all of
//! which are documented in detail for the respective macro.
use metrics::{
    counter, decrement_gauge, gauge, histogram, increment_counter, increment_gauge,
    register_counter, register_gauge, register_histogram, GaugeValue, Key, Recorder, Unit,
};

#[derive(Default)]
struct PrintRecorder;

impl Recorder for PrintRecorder {
    fn register_counter(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        println!(
            "(counter) registered key {} with unit {:?} and description {:?}",
            key, unit, description
        );
    }

    fn register_gauge(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        println!(
            "(gauge) registered key {} with unit {:?} and description {:?}",
            key, unit, description
        );
    }

    fn register_histogram(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        println!(
            "(histogram) registered key {} with unit {:?} and description {:?}",
            key, unit, description
        );
    }

    fn increment_counter(&self, key: &Key, value: u64) {
        println!("(counter) got value {} for key {}", value, key);
    }

    fn update_gauge(&self, key: &Key, value: GaugeValue) {
        println!("(gauge) got value {:?} for key {}", value, key);
    }

    fn record_histogram(&self, key: &Key, value: f64) {
        println!("(histogram) got value {} for key {}", value, key);
    }
}

fn init_print_logger() {
    let recorder = PrintRecorder::default();
    metrics::set_boxed_recorder(Box::new(recorder)).unwrap()
}

fn main() {
    let server_name = "web03".to_string();

    init_print_logger();

    let common_labels = &[("listener", "frontend")];

    // Go through registration:
    register_counter!("requests_processed", "number of requests processed");
    register_counter!("bytes_sent", Unit::Bytes);
    register_gauge!("connection_count", common_labels);
    register_histogram!(
        "svc.execution_time",
        Unit::Milliseconds,
        "execution time of request handler"
    );
    register_gauge!("unused_gauge", "service" => "backend");
    register_histogram!("unused_histogram", Unit::Seconds, "unused histo", "service" => "middleware");

    // All the supported permutations of `increment!`:
    increment_counter!("requests_processed");
    increment_counter!("requests_processed", "request_type" => "admin");
    increment_counter!("requests_processed", "request_type" => "admin", "server" => server_name.clone());
    increment_counter!("requests_processed", common_labels);

    // All the supported permutations of `counter!`:
    counter!("bytes_sent", 64);
    counter!("bytes_sent", 64, "listener" => "frontend");
    counter!("bytes_sent", 64, "listener" => "frontend", "server" => server_name.clone());
    counter!("bytes_sent", 64, common_labels);

    // All the supported permutations of `gauge!` and its increment/decrement versions:
    gauge!("connection_count", 300.0);
    gauge!("connection_count", 300.0, "listener" => "frontend");
    gauge!("connection_count", 300.0, "listener" => "frontend", "server" => server_name.clone());
    gauge!("connection_count", 300.0, common_labels);
    increment_gauge!("connection_count", 300.0);
    increment_gauge!("connection_count", 300.0, "listener" => "frontend");
    increment_gauge!("connection_count", 300.0, "listener" => "frontend", "server" => server_name.clone());
    increment_gauge!("connection_count", 300.0, common_labels);
    decrement_gauge!("connection_count", 300.0);
    decrement_gauge!("connection_count", 300.0, "listener" => "frontend");
    decrement_gauge!("connection_count", 300.0, "listener" => "frontend", "server" => server_name.clone());
    decrement_gauge!("connection_count", 300.0, common_labels);

    // All the supported permutations of `histogram!`:
    histogram!("svc.execution_time", 70.0);
    histogram!("svc.execution_time", 70.0, "type" => "users");
    histogram!("svc.execution_time", 70.0, "type" => "users", "server" => server_name.clone());
    histogram!("svc.execution_time", 70.0, common_labels);
}
