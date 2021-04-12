#[macro_use]
extern crate criterion;

use criterion::Criterion;

use metrics::{counter, GaugeValue, Key, Recorder, Unit};
use rand::{thread_rng, Rng};

#[derive(Default)]
struct TestRecorder;
impl Recorder for TestRecorder {
    fn register_counter(
        &self,
        _key: &Key,
        _unit: Option<Unit>,
        _description: Option<&'static str>,
    ) {
    }
    fn register_gauge(&self, _key: &Key, _unit: Option<Unit>, _description: Option<&'static str>) {}
    fn register_histogram(
        &self,
        _key: &Key,
        _unit: Option<Unit>,
        _description: Option<&'static str>,
    ) {
    }
    fn increment_counter(&self, _key: &Key, _value: u64) {}
    fn update_gauge(&self, _key: &Key, _value: GaugeValue) {}
    fn record_histogram(&self, _key: &Key, _value: f64) {}
}

fn reset_recorder() {
    let recorder = unsafe { &*Box::into_raw(Box::new(TestRecorder::default())) };
    unsafe { metrics::set_recorder_racy(recorder).unwrap() }
}

fn macro_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("macros");
    group.bench_function("uninitialized/no_labels", |b| {
        metrics::clear_recorder();
        b.iter(|| {
            counter!("counter_bench", 42);
        })
    });
    group.bench_function("uninitialized/with_static_labels", |b| {
        metrics::clear_recorder();
        b.iter(|| {
            counter!("counter_bench", 42, "request" => "http", "svc" => "admin");
        })
    });
    group.bench_function("initialized/no_labels", |b| {
        reset_recorder();
        b.iter(|| {
            counter!("counter_bench", 42);
        });
        metrics::clear_recorder();
    });
    group.bench_function("initialized/with_static_labels", |b| {
        reset_recorder();
        b.iter(|| {
            counter!("counter_bench", 42, "request" => "http", "svc" => "admin");
        });
        metrics::clear_recorder();
    });
    group.bench_function("initialized/with_dynamic_labels", |b| {
        let label_val = thread_rng().gen::<u64>().to_string();

        reset_recorder();
        b.iter(move || {
            counter!("counter_bench", 42, "request" => "http", "uid" => label_val.clone());
        });
        metrics::clear_recorder();
    });
    group.finish();
}

criterion_group!(benches, macro_benchmark);
criterion_main!(benches);
