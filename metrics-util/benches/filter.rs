use criterion::{criterion_group, criterion_main, Criterion};

#[cfg(feature = "layer-filter")]
use metrics::{Key, Label, NoopRecorder, Recorder};

#[cfg(feature = "layer-filter")]
use metrics_util::layers::{FilterLayer, Layer};

#[allow(unused_variables)]
fn layer_benchmark(c: &mut Criterion) {
    #[cfg(feature = "layer-filter")]
    {
        let mut group = c.benchmark_group("filter");
        group.bench_function("match", |b| {
            let patterns = vec!["tokio"];
            let filter_layer = FilterLayer::from_patterns(patterns);
            let recorder = filter_layer.layer(NoopRecorder);
            static KEY_NAME: &'static str = "tokio.foo";
            static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
            static KEY_DATA: Key = Key::from_static_parts(&KEY_NAME, &KEY_LABELS);

            b.iter(|| {
                let _ = recorder.register_counter(&KEY_DATA);
            })
        });
        group.bench_function("no match", |b| {
            let patterns = vec!["tokio"];
            let filter_layer = FilterLayer::from_patterns(patterns);
            let recorder = filter_layer.layer(NoopRecorder);
            static KEY_NAME: &'static str = "hyper.foo";
            static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
            static KEY_DATA: Key = Key::from_static_parts(&KEY_NAME, &KEY_LABELS);

            b.iter(|| {
                let _ = recorder.register_counter(&KEY_DATA);
            })
        });
        group.bench_function("noop recorder overhead (increment_counter)", |b| {
            let recorder = NoopRecorder;
            static KEY_NAME: &'static str = "tokio.foo";
            static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
            static KEY_DATA: Key = Key::from_static_parts(&KEY_NAME, &KEY_LABELS);

            b.iter(|| {
                let _ = recorder.register_counter(&KEY_DATA);
            })
        });
    }
}

criterion_group!(benches, layer_benchmark);
criterion_main!(benches);
