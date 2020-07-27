use std::sync::atomic::{AtomicUsize, Ordering};

use metrics::{counter, gauge, histogram, increment, Identifier, Key, Recorder};

#[allow(dead_code)]
static RECORDER: PrintRecorder = PrintRecorder::new();

#[derive(Default)]
struct PrintRecorder(AtomicUsize);

impl PrintRecorder {
    pub const fn new() -> PrintRecorder {
        PrintRecorder(AtomicUsize::new(0))
    }
}

impl Recorder for PrintRecorder {
    fn register_counter(&self, key: Key, _description: Option<&'static str>) -> Identifier {
        let id = self.0.fetch_add(1, Ordering::SeqCst);
        println!("(counter) mapping key {} to id {}", key, id);
        id.into()
    }

    fn register_gauge(&self, key: Key, _description: Option<&'static str>) -> Identifier {
        let id = self.0.fetch_add(1, Ordering::SeqCst);
        println!("(gauge) mapping key {} to id {}", key, id);
        id.into()
    }

    fn register_histogram(&self, key: Key, _description: Option<&'static str>) -> Identifier {
        let id = self.0.fetch_add(1, Ordering::SeqCst);
        println!("(histogram) mapping key {} to id {}", key, id);
        id.into()
    }

    fn increment_counter(&self, id: Identifier, value: u64) {
        println!("(counter) got value {} for id {:?}", value, id);
    }

    fn update_gauge(&self, id: Identifier, value: f64) {
        println!("(gauge) got value {} for id {:?}", value, id);
    }

    fn record_histogram(&self, id: Identifier, value: u64) {
        println!("(histogram) got value {} for id {:?}", value, id);
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
