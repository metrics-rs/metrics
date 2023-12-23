#[macro_use]
extern crate criterion;

use criterion::Criterion;

use metrics::{
    counter, Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit,
};
use rand::{thread_rng, Rng};

#[derive(Default)]
struct TestRecorder;
impl Recorder for TestRecorder {
    fn describe_counter(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
    fn describe_gauge(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
    fn describe_histogram(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
    fn register_counter(&self, _: &Key, _: &Metadata<'_>) -> Counter {
        Counter::noop()
    }
    fn register_gauge(&self, _: &Key, _: &Metadata<'_>) -> Gauge {
        Gauge::noop()
    }
    fn register_histogram(&self, _: &Key, _: &Metadata<'_>) -> Histogram {
        Histogram::noop()
    }
}

fn macro_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("macros");
    group.bench_function("uninitialized/no_labels", |b| {
        b.iter(|| {
            counter!("counter_bench").increment(42);
        })
    });
    group.bench_function("uninitialized/with_static_labels", |b| {
        b.iter(|| {
            counter!("counter_bench", "request" => "http", "svc" => "admin").increment(42);
        })
    });
    group.bench_function("global_initialized/no_labels", |b| {
        let _ = metrics::set_global_recorder(TestRecorder::default());
        b.iter(|| {
            counter!("counter_bench").increment(42);
        });
    });
    group.bench_function("global_initialized/with_static_labels", |b| {
        let _ = metrics::set_global_recorder(TestRecorder::default());
        b.iter(|| {
            counter!("counter_bench", "request" => "http", "svc" => "admin").increment(42);
        });
    });
    group.bench_function("global_initialized/with_dynamic_labels", |b| {
        let _ = metrics::set_global_recorder(TestRecorder::default());

        let label_val = thread_rng().gen::<u64>().to_string();
        b.iter(move || {
            counter!("counter_bench", "request" => "http", "uid" => label_val.clone())
                .increment(42);
        });
    });
    group.bench_function("local_initialized/no_labels", |b| {
        let recorder = TestRecorder::default();

        metrics::with_local_recorder(&recorder, || {
            b.iter(|| {
                counter!("counter_bench").increment(42);
            });
        });
    });
    group.bench_function("local_initialized/with_static_labels", |b| {
        let recorder = TestRecorder::default();

        metrics::with_local_recorder(&recorder, || {
            b.iter(|| {
                counter!("counter_bench", "request" => "http", "svc" => "admin").increment(42);
            });
        });
    });
    group.bench_function("local_initialized/with_dynamic_labels", |b| {
        let recorder = TestRecorder::default();

        metrics::with_local_recorder(&recorder, || {
            let label_val = thread_rng().gen::<u64>().to_string();
            b.iter(move || {
                counter!("counter_bench", "request" => "http", "uid" => label_val.clone())
                    .increment(42);
            });
        });
    });
    group.finish();
}

criterion_group!(benches, macro_benchmark);
criterion_main!(benches);
