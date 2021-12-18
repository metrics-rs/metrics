use criterion::{criterion_group, criterion_main, Criterion};
use metrics::{Key, Label, NoopRecorder, Recorder};
use metrics_util::layers::{Layer, PrefixLayer};

fn layer_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("prefix");
    group.bench_function("basic", |b| {
        let prefix_layer = PrefixLayer::new("prefix");
        let recorder = prefix_layer.layer(NoopRecorder);
        static KEY_NAME: &'static str = "simple_key";
        static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
        static KEY_DATA: Key = Key::from_static_parts(&KEY_NAME, &KEY_LABELS);

        b.iter(|| {
            let _ = recorder.register_counter(&KEY_DATA);
        })
    });
    group.bench_function("noop recorder overhead (increment_counter)", |b| {
        let recorder = NoopRecorder;
        static KEY_NAME: &'static str = "simple_key";
        static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
        static KEY_DATA: Key = Key::from_static_parts(&KEY_NAME, &KEY_LABELS);

        b.iter(|| {
            let _ = recorder.register_counter(&KEY_DATA);
        })
    });
    group.finish();
}

criterion_group!(benches, layer_benchmark);
criterion_main!(benches);
