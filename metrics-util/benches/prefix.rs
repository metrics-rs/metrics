use criterion::{criterion_group, criterion_main, Benchmark, Criterion};
use metrics::{Key, KeyData, Label, NameParts, NoopRecorder, Recorder};
use metrics_util::layers::{Layer, PrefixLayer};

fn layer_benchmark(c: &mut Criterion) {
    c.bench(
        "prefix",
        Benchmark::new("basic", |b| {
            let prefix_layer = PrefixLayer::new("prefix");
            let recorder = prefix_layer.layer(NoopRecorder);
            static NAME: NameParts = NameParts::from_static_name("key");
            static LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
            static KEY_DATA: KeyData = KeyData::from_static_parts(&NAME, &LABELS);

            b.iter(|| {
                recorder.increment_counter(Key::Borrowed(&KEY_DATA), 1);
            })
        })
        .with_function("noop recorder overhead (increment_counter)", |b| {
            let recorder = NoopRecorder;
            static NAME: NameParts = NameParts::from_static_name("key");
            static LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
            static KEY_DATA: KeyData = KeyData::from_static_parts(&NAME, &LABELS);

            b.iter(|| {
                recorder.increment_counter(Key::Borrowed(&KEY_DATA), 1);
            })
        }),
    );
}

criterion_group!(benches, layer_benchmark);
criterion_main!(benches);
