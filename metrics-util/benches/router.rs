use criterion::{criterion_group, criterion_main, Criterion};

use metrics::{Key, NoopRecorder, Recorder};
use metrics_util::layers::RouterBuilder;
use metrics_util::MetricKindMask;

fn layer_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("router");
    group.bench_function("default target (via mask)", |b| {
        let recorder = RouterBuilder::from_recorder(NoopRecorder).build();
        let key = Key::from_name("test_key");
        static METADATA: metrics::Metadata =
            metrics::Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));

        b.iter(|| {
            let _ = recorder.register_counter(&key, &METADATA);
        })
    });
    group.bench_function("default target (via fallback)", |b| {
        let mut builder = RouterBuilder::from_recorder(NoopRecorder);
        builder.add_route(MetricKindMask::COUNTER, "override", NoopRecorder);
        let recorder = builder.build();
        let key = Key::from_name("normal_key");
        static METADATA: metrics::Metadata =
            metrics::Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));

        b.iter(|| {
            let _ = recorder.register_counter(&key, &METADATA);
        })
    });
    group.bench_function("routed target", |b| {
        let mut builder = RouterBuilder::from_recorder(NoopRecorder);
        builder.add_route(MetricKindMask::COUNTER, "override", NoopRecorder);
        let recorder = builder.build();
        let key = Key::from_name("override_key");
        static METADATA: metrics::Metadata =
            metrics::Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));

        b.iter(|| {
            let _ = recorder.register_counter(&key, &METADATA);
        })
    });
}

criterion_group!(benches, layer_benchmark);
criterion_main!(benches);
