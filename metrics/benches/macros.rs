#[macro_use]
extern crate criterion;

use criterion::Criterion;

use metrics::{counter, Counter, Gauge, Histogram, Key, KeyName, Recorder, SharedString, Unit};
use rand::{thread_rng, Rng};

#[derive(Default)]
struct TestRecorder;
impl Recorder for TestRecorder {
    fn describe_counter(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
    fn describe_gauge(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
    fn describe_histogram(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
    fn register_counter(&self, _: &Key) -> Counter {
        Counter::noop()
    }
    fn register_gauge(&self, _: &Key) -> Gauge {
        Gauge::noop()
    }
    fn register_histogram(&self, _: &Key) -> Histogram {
        Histogram::noop()
    }
}

fn reset_recorder() {
    unsafe {
        metrics::clear_recorder();
    }
    metrics::set_boxed_recorder(Box::new(TestRecorder::default())).unwrap()
}

fn macro_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("macros");
    group.bench_function("uninitialized/no_labels", |b| {
        unsafe {
            metrics::clear_recorder();
        }
        b.iter(|| {
            counter!("counter_bench", 42);
        })
    });
    group.bench_function("uninitialized/with_static_labels", |b| {
        unsafe {
            metrics::clear_recorder();
        }
        b.iter(|| {
            counter!("counter_bench", 42, "request" => "http", "svc" => "admin");
        })
    });
    group.bench_function("initialized/no_labels", |b| {
        reset_recorder();
        b.iter(|| {
            counter!("counter_bench", 42);
        });
        unsafe {
            metrics::clear_recorder();
        }
    });
    group.bench_function("initialized/with_static_labels", |b| {
        reset_recorder();
        b.iter(|| {
            counter!("counter_bench", 42, "request" => "http", "svc" => "admin");
        });
        unsafe {
            metrics::clear_recorder();
        }
    });
    group.bench_function("initialized/with_dynamic_labels", |b| {
        let label_val = thread_rng().gen::<u64>().to_string();

        reset_recorder();
        b.iter(move || {
            counter!("counter_bench", 42, "request" => "http", "uid" => label_val.clone());
        });
        unsafe {
            metrics::clear_recorder();
        }
    });
    group.finish();
}

criterion_group!(benches, macro_benchmark);
criterion_main!(benches);
