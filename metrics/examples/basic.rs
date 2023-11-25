//! This example is part unit test and part demonstration.
//!
//! We show all of the registration macros, as well as all of the "emission" macros, the ones you
//! would actually call to update a metric.
//!
//! We demonstrate the various permutations of values that can be passed in the macro calls, all of
//! which are documented in detail for the respective macro.
use std::sync::Arc;

use metrics::{
    counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram, KeyName,
    Metadata, SharedString,
};
use metrics::{Counter, CounterFn, Gauge, GaugeFn, Histogram, HistogramFn, Key, Recorder, Unit};

struct PrintHandle(Key);

impl CounterFn for PrintHandle {
    fn increment(&self, value: u64) {
        println!("counter increment for '{}': {}", self.0, value);
    }

    fn absolute(&self, value: u64) {
        println!("counter absolute for '{}': {}", self.0, value);
    }
}

impl GaugeFn for PrintHandle {
    fn increment(&self, value: f64) {
        println!("gauge increment for '{}': {}", self.0, value);
    }

    fn decrement(&self, value: f64) {
        println!("gauge decrement for '{}': {}", self.0, value);
    }

    fn set(&self, value: f64) {
        println!("gauge set for '{}': {}", self.0, value);
    }
}

impl HistogramFn for PrintHandle {
    fn record(&self, value: f64) {
        println!("histogram record for '{}': {}", self.0, value);
    }
}

#[derive(Default)]
struct PrintRecorder;

impl Recorder for PrintRecorder {
    fn describe_counter(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        println!(
            "(counter) registered key {} with unit {:?} and description {:?}",
            key_name.as_str(),
            unit,
            description
        );
    }

    fn describe_gauge(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        println!(
            "(gauge) registered key {} with unit {:?} and description {:?}",
            key_name.as_str(),
            unit,
            description
        );
    }

    fn describe_histogram(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        println!(
            "(histogram) registered key {} with unit {:?} and description {:?}",
            key_name.as_str(),
            unit,
            description
        );
    }

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        Counter::from_arc(Arc::new(PrintHandle(key.clone())))
    }

    fn register_gauge(&self, key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        Gauge::from_arc(Arc::new(PrintHandle(key.clone())))
    }

    fn register_histogram(&self, key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        Histogram::from_arc(Arc::new(PrintHandle(key.clone())))
    }
}

fn init_print_logger() {
    let recorder = PrintRecorder::default();
    metrics::set_global_recorder(recorder).unwrap()
}

fn main() {
    let server_name = "web03".to_string();

    init_print_logger();

    let common_labels = &[("listener", "frontend")];

    // Go through describing the metrics:
    describe_counter!("requests_processed", "number of requests processed");
    describe_counter!("bytes_sent", Unit::Bytes, "total number of bytes sent");
    describe_gauge!("connection_count", "current number of client connections");
    describe_histogram!(
        "svc.execution_time",
        Unit::Milliseconds,
        "execution time of request handler"
    );
    describe_gauge!("unused_gauge", "some gauge we'll never use in this program");
    describe_histogram!(
        "unused_histogram",
        Unit::Seconds,
        "some histogram we'll also never use in this program"
    );

    // And registering them:
    let counter1 = counter!("test_counter");
    counter1.increment(1);
    let counter2 = counter!("test_counter", "type" => "absolute");
    counter2.absolute(42);

    let gauge1 = gauge!("test_gauge");
    gauge1.increment(1.0);
    let gauge2 = gauge!("test_gauge", "type" => "decrement");
    gauge2.decrement(1.0);
    let gauge3 = gauge!("test_gauge", "type" => "set");
    gauge3.set(3.1459);

    let histogram1 = histogram!("test_histogram");
    histogram1.record(0.57721);

    // All the supported permutations of `counter!` and its increment/absolute versions:
    counter!("bytes_sent").increment(64);
    counter!("bytes_sent", "listener" => "frontend").increment(64);
    counter!("bytes_sent", "listener" => "frontend", "server" => server_name.clone()).increment(64);
    counter!("bytes_sent", common_labels).increment(64);

    counter!("requests_processed").increment(1);
    counter!("requests_processed", "request_type" => "admin").increment(1);
    counter!("requests_processed", "request_type" => "admin", "server" => server_name.clone())
        .increment(1);
    counter!("requests_processed", common_labels).increment(1);

    counter!("bytes_sent").absolute(64);
    counter!("bytes_sent", "listener" => "frontend").absolute(64);
    counter!("bytes_sent", "listener" => "frontend", "server" => server_name.clone()).absolute(64);
    counter!("bytes_sent", common_labels).absolute(64);

    // All the supported permutations of `gauge!` and its increment/decrement versions:
    gauge!("connection_count").set(300.0);
    gauge!("connection_count", "listener" => "frontend").set(300.0);
    gauge!("connection_count", "listener" => "frontend", "server" => server_name.clone())
        .set(300.0);
    gauge!("connection_count", common_labels).set(300.0);
    gauge!("connection_count").increment(300.0);
    gauge!("connection_count", "listener" => "frontend").increment(300.0);
    gauge!("connection_count", "listener" => "frontend", "server" => server_name.clone())
        .increment(300.0);
    gauge!("connection_count", common_labels).increment(300.0);
    gauge!("connection_count").decrement(300.0);
    gauge!("connection_count", "listener" => "frontend").decrement(300.0);
    gauge!("connection_count", "listener" => "frontend", "server" => server_name.clone())
        .decrement(300.0);
    gauge!("connection_count", common_labels).decrement(300.0);

    // All the supported permutations of `histogram!`:
    histogram!("svc.execution_time").record(70.0);
    histogram!("svc.execution_time", "type" => "users").record(70.0);
    histogram!("svc.execution_time", "type" => "users", "server" => server_name.clone())
        .record(70.0);
    histogram!("svc.execution_time", common_labels).record(70.0);
}
