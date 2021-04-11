use criterion::{criterion_group, criterion_main, Benchmark, Criterion};
use metrics::{Key, Label, NoopRecorder, Recorder, SharedString};
use metrics_util::layers::{Layer, PrefixLayer};

fn layer_benchmark(c: &mut Criterion) {
    c.bench(
        "prefix",
        Benchmark::new("basic", |b| {
            let prefix_layer = PrefixLayer::new("prefix");
            let recorder = prefix_layer.layer(NoopRecorder);
            static KEY_NAME: [SharedString; 1] = [SharedString::const_str("simple_key")];
            static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
            static KEY_DATA: Key = Key::from_static_parts(&KEY_NAME, &KEY_LABELS);

            b.iter(|| {
                recorder.increment_counter(&KEY_DATA, 1);
            })
        })
        .with_function("noop recorder overhead (increment_counter)", |b| {
            let recorder = NoopRecorder;
            static KEY_NAME: [SharedString; 1] = [SharedString::const_str("simple_key")];
            static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
            static KEY_DATA: Key = Key::from_static_parts(&KEY_NAME, &KEY_LABELS);

            b.iter(|| {
                recorder.increment_counter(&KEY_DATA, 1);
            })
        }),
    );
}

criterion_group!(benches, layer_benchmark);
criterion_main!(benches);
