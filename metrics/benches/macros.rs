#[macro_use]
extern crate criterion;

use criterion::{Benchmark, Criterion};

use metrics::{counter, Key, Recorder};
use rand::{thread_rng, Rng};

#[derive(Default)]
struct TestRecorder;
impl Recorder for TestRecorder {
    fn register_counter(&self, _key: Key, _description: Option<&'static str>) {}
    fn register_gauge(&self, _key: Key, _description: Option<&'static str>) {}
    fn register_histogram(&self, _key: Key, _description: Option<&'static str>) {}
    fn increment_counter(&self, _key: Key, _value: u64) {}
    fn update_gauge(&self, _key: Key, _value: f64) {}
    fn record_histogram(&self, _key: Key, _value: u64) {}
}

fn reset_recorder() {
    let recorder = unsafe { &*Box::into_raw(Box::new(TestRecorder::default())) };
    unsafe { metrics::set_recorder_racy(recorder).unwrap() }
}

fn macro_benchmark(c: &mut Criterion) {
    c.bench(
        "macros",
        Benchmark::new("uninitialized/no_labels", |b| {
            b.iter(|| {
                counter!("counter_bench", 42);
            })
        })
        .with_function("uninitialized/with_static_labels", |b| {
            b.iter(|| {
                counter!("counter_bench", 42, "request" => "http", "svc" => "admin");
            })
        })
        .with_function("initialized/no_labels", |b| {
            reset_recorder();
            b.iter(|| {
                counter!("counter_bench", 42);
            });
            metrics::clear_recorder();
        })
        .with_function("initialized/with_static_labels", |b| {
            reset_recorder();
            b.iter(|| {
                counter!("counter_bench", 42, "request" => "http", "svc" => "admin");
            });
            metrics::clear_recorder();
        })
        .with_function("initialized/with_dynamic_labels", |b| {
            let label_val = thread_rng().gen::<u64>().to_string();

            reset_recorder();
            b.iter(move || {
                counter!("counter_bench", 42, "request" => "http", "uid" => label_val.clone());
            });
            metrics::clear_recorder();
        }),
    );
}

criterion_group!(benches, macro_benchmark);
criterion_main!(benches);
