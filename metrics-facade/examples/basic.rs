#[macro_use]
extern crate metrics_facade;

use metrics_facade::Recorder;
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
    metrics_facade::set_boxed_recorder(Box::new(recorder)).unwrap()
}

#[cfg(not(feature = "std"))]
fn init_print_logger() {
    metrics_facade::set_recorder(&RECORDER).unwrap()
}

fn main() {
    init_print_logger();
    counter!("mycounter", 42);
    gauge!("mygauge", 123);
    timing!("mytiming", 120, 190);
    timing!("mytiming", 70);
    value!("myvalue", 666);
}
