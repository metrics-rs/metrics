use criterion::{criterion_group, criterion_main, Criterion};

#[cfg(feature = "layer-router")]
use metrics::{Key, NoopRecorder, Recorder};

#[cfg(feature = "layer-router")]
use metrics_util::layers::RouterBuilder;
use metrics_util::MetricKindMask;

#[allow(unused_variables)]
fn layer_benchmark(c: &mut Criterion) {
    #[cfg(feature = "layer-router")]
    {
        let mut group = c.benchmark_group("router");
        group.bench_function("default target (via mask)", |b| {
            let recorder = RouterBuilder::from_recorder(NoopRecorder).build();
            let key = Key::from_name("test_key");

            b.iter(|| {
                recorder.increment_counter(&key, 1);
            })
        });
        group.bench_function("default target (via fallback)", |b| {
            let mut builder = RouterBuilder::from_recorder(NoopRecorder);
            builder.add_route(MetricKindMask::COUNTER, "override", NoopRecorder);
            let recorder = builder.build();
            let key = Key::from_name("normal_key");

            b.iter(|| {
                recorder.increment_counter(&key, 1);
            })
        });
        group.bench_function("routed target", |b| {
            let mut builder = RouterBuilder::from_recorder(NoopRecorder);
            builder.add_route(MetricKindMask::COUNTER, "override", NoopRecorder);
            let recorder = builder.build();
            let key = Key::from_name("override_key");

            b.iter(|| {
                recorder.increment_counter(&key, 1);
            })
        });
    }
}

criterion_group!(benches, layer_benchmark);
criterion_main!(benches);
