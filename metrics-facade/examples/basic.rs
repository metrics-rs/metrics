#[macro_use]
extern crate metrics_facade;

use metrics_facade::MetricsRecorder;

#[derive(Default)]
struct PrintRecorder;

impl MetricsRecorder for PrintRecorder {
    fn enabled(&self) -> bool { true }

    fn record_counter(&self, key: &str, value: u64) {
        println!("metrics -> counter(name={}, value={})", key, value);
    }

    fn record_gauge(&self, key: &str, value: i64) {
        println!("metrics -> gauge(name={}, value={})", key, value);
    }

    fn record_histogram(&self, key: &str, value: u64) {
        println!("metrics -> histogram(name={}, value={})", key, value);
    }
}

fn init_print_logger() {
    let recorder = PrintRecorder::default();
    metrics_facade::set_boxed_recorder(Box::new(recorder)).unwrap()
}

fn main() {
    init_print_logger();
    counter!("mycounter", 42);
    gauge!("mygauge", 123);
    timing!("mytiming", 120, 190);
    timing!("mytiming", 70);
    value!("myvalue", 666);
}
