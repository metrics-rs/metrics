use metrics::{counter, gauge, histogram, increment, Key, Recorder};

#[allow(dead_code)]
static RECORDER: PrintRecorder = PrintRecorder;

#[derive(Default)]
struct PrintRecorder;

impl Recorder for PrintRecorder {
    fn register_counter(&self, key: Key, description: Option<&'static str>) {
        println!(
            "(counter) registered key {} with description {:?}",
            key, description
        );
    }

    fn register_gauge(&self, key: Key, description: Option<&'static str>) {
        println!(
            "(gauge) registered key {} with description {:?}",
            key, description
        );
    }

    fn register_histogram(&self, key: Key, description: Option<&'static str>) {
        println!(
            "(histogram) registered key {} with description {:?}",
            key, description
        );
    }

    fn increment_counter(&self, key: Key, value: u64) {
        println!("(counter) got value {} for key {}", value, key);
    }

    fn update_gauge(&self, key: Key, value: f64) {
        println!("(gauge) got value {} for key {}", value, key);
    }

    fn record_histogram(&self, key: Key, value: u64) {
        println!("(histogram) got value {} for key {}", value, key);
    }
}

#[cfg(feature = "std")]
fn init_print_logger() {
    let recorder = PrintRecorder::default();
    metrics::set_boxed_recorder(Box::new(recorder)).unwrap()
}

#[cfg(not(feature = "std"))]
fn init_print_logger() {
    metrics::set_recorder(&RECORDER).unwrap()
}

fn main() {
    let server_name = "web03".to_string();

    init_print_logger();
    for _ in 0..3 {
        increment!("requests_processed");
        increment!("requests_processed", "request_type" => "admin");
    }
    increment!("requests_processed", "request_type" => "admin", "server" => server_name.clone());
    counter!("requests_processed", 1);
    counter!("requests_processed", 1, "request_type" => "admin");
    counter!("requests_processed", 1, "request_type" => "admin", "server" => server_name.clone());
    gauge!("connection_count", 300.0);
    gauge!("connection_count", 300.0, "listener" => "frontend");
    gauge!("connection_count", 300.0, "listener" => "frontend", "server" => server_name.clone());
    histogram!("service.execution_time", 70);
    histogram!("service.execution_time", 70, "type" => "users");
    histogram!("service.execution_time", 70, "type" => "users", "server" => server_name.clone());
    histogram!(<"service.execution_time">, 70);
    histogram!(<"service.execution_time">, 70, "type" => "users");
    histogram!(<"service.execution_time">, 70, "type" => "users", "server" => server_name.clone());
}
