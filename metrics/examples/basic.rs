#[macro_use]
extern crate metrics;

use metrics::Recorder;
use metrics_core::Key;

#[allow(dead_code)]
static RECORDER: PrintRecorder = PrintRecorder;

#[derive(Default)]
struct PrintRecorder;

impl Recorder for PrintRecorder {
    fn record_counter(&self, key: Key, value: u64) {
        println!("metrics -> counter(name={}, value={})", key, value);
    }

    fn record_gauge(&self, key: Key, value: i64) {
        println!("metrics -> gauge(name={}, value={})", key, value);
    }

    fn record_histogram(&self, key: Key, value: u64) {
        println!("metrics -> histogram(name={}, value={})", key, value);
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
    init_print_logger();
    counter!("requests_processed", 1);
    counter!("requests_processed", 1, "request_type" => "admin");
    gauge!("connection_count", 300);
    gauge!("connection_count", 300, "listener" => "frontend");
    timing!("service.execution_time", 120, 190);
    timing!("service.execution_time", 120, 190, "type" => "users");
    timing!("service.execution_time", 70);
    timing!("service.execution_time", 70, "type" => "users");
    value!("service.results_returned", 666);
    value!("service.results_returned", 666, "type" => "users");
}
