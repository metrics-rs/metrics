#[macro_use]
extern crate criterion;

use criterion::Criterion;
use std::sync::Arc;

use metrics::{
    counter, Counter, Gauge, Histogram, Key, KeyName, Label, Metadata, Recorder, SharedString, Unit,
};

#[derive(Debug)]
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
        let _ = metrics::set_global_recorder(TestRecorder);
        b.iter(|| {
            counter!("counter_bench").increment(42);
        });
    });
    group.bench_function("global_initialized/with_static_labels", |b| {
        let _ = metrics::set_global_recorder(TestRecorder);
        b.iter(|| {
            counter!("counter_bench", "request" => "http", "svc" => "admin").increment(42);
        });
    });
    group.bench_function("global_initialized/with_dynamic_labels", |b| {
        let _ = metrics::set_global_recorder(TestRecorder);

        let label_val = rand::random::<u64>().to_string();
        b.iter(move || {
            counter!("counter_bench", "request" => "http", "uid" => label_val.clone())
                .increment(42);
        });
    });
    group.bench_function("local_initialized/no_labels", |b| {
        metrics::with_local_recorder(&TestRecorder, || {
            b.iter(|| {
                counter!("counter_bench").increment(42);
            });
        });
    });
    group.bench_function("local_initialized/with_static_labels", |b| {
        metrics::with_local_recorder(&TestRecorder, || {
            b.iter(|| {
                counter!("counter_bench", "request" => "http", "svc" => "admin").increment(42);
            });
        });
    });
    group.bench_function("local_initialized/with_dynamic_labels", |b| {
        metrics::with_local_recorder(&TestRecorder, || {
            let label_val = rand::random::<u64>().to_string();
            b.iter(move || {
                counter!("counter_bench", "request" => "http", "uid" => label_val.clone())
                    .increment(42);
            });
        });
    });
    group.finish();
}

fn shared_labels_benchmark(c: &mut Criterion) {
    // Compare seven keys with sixteen labels each. Share the strings in both cases to isolate
    // the cost of copying and retaining the label arrays.
    let label_names = [
        "label_0", "label_1", "label_2", "label_3", "label_4", "label_5", "label_6", "label_7",
        "label_8", "label_9", "label_10", "label_11", "label_12", "label_13", "label_14",
        "label_15",
    ];
    let labels: Vec<Label> = label_names
        .iter()
        .enumerate()
        .map(|(i, name)| Label::new(*name, Arc::<str>::from(i.to_string())))
        .collect();
    let shared: Arc<[Label]> = labels.clone().into();
    let names =
        ["metric_0", "metric_1", "metric_2", "metric_3", "metric_4", "metric_5", "metric_6"];
    let mut group = c.benchmark_group("shared_labels/retain_seven_keys");

    group.bench_function("owned", |b| {
        b.iter(|| {
            names
                .iter()
                .map(|name| Key::from_parts(*name, labels.clone()).to_retained())
                .collect::<Vec<_>>()
        });
    });
    group.bench_function("shared", |b| {
        b.iter(|| {
            names
                .iter()
                .map(|name| Key::from_parts(*name, Arc::clone(&shared)).to_retained())
                .collect::<Vec<_>>()
        });
    });
    group.finish();
}

criterion_group!(benches, macro_benchmark, shared_labels_benchmark);
criterion_main!(benches);
