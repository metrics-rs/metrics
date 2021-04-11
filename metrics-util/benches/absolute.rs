use criterion::{criterion_group, criterion_main, Criterion};

#[cfg(feature = "layer-absolute")]
use metrics::{Key, Label, NoopRecorder, Recorder, SharedString};

#[cfg(feature = "layer-absolute")]
use metrics_util::layers::{AbsoluteLayer, Layer};

#[allow(unused_variables)]
fn layer_benchmark(c: &mut Criterion) {
    #[cfg(feature = "layer-absolute")]
    {
        let mut group = c.benchmark_group("Absolute");
        group.bench_function("no match", |b| {
            let patterns = vec!["rdkafka"];
            let absolute_layer = AbsoluteLayer::from_patterns(patterns);
            let recorder = absolute_layer.layer(NoopRecorder);
            static KEY_NAME: [SharedString; 1] = [SharedString::const_str("counter")];
            static KEY_DATA: Key = Key::from_static_name(&KEY_NAME);

            b.iter(|| {
                recorder.increment_counter(&KEY_DATA, 1);
            })
        });
        group.bench_function("match (same value)", |b| {
            let patterns = vec!["rdkafka"];
            let absolute_layer = AbsoluteLayer::from_patterns(patterns);
            let recorder = absolute_layer.layer(NoopRecorder);
            static KEY_NAME: [SharedString; 1] = [SharedString::const_str("rdkafka.bytes")];
            static KEY_DATA: Key = Key::from_static_name(&KEY_NAME);

            b.iter(|| {
                recorder.increment_counter(&KEY_DATA, 1);
            })
        });
        group.bench_function("match (updating value)", |b| {
            let patterns = vec!["tokio"];
            let absolute_layer = AbsoluteLayer::from_patterns(patterns);
            let recorder = absolute_layer.layer(NoopRecorder);
            static KEY_NAME: [SharedString; 1] = [SharedString::const_str("rdkafka.bytes")];
            static KEY_DATA: Key = Key::from_static_name(&KEY_NAME);

            let mut counter = 1;

            b.iter(|| {
                recorder.increment_counter(&KEY_DATA, counter);
                counter += 1;
            })
        });
        group.bench_function("noop recorder overhead (increment_counter)", |b| {
            let recorder = NoopRecorder;
            static KEY_NAME: [SharedString; 1] = [SharedString::const_str("counter")];
            static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
            static KEY_DATA: Key = Key::from_static_parts(&KEY_NAME, &KEY_LABELS);

            b.iter(|| {
                recorder.increment_counter(&KEY_DATA, 1);
            })
        });
    }
}

criterion_group!(benches, layer_benchmark);
criterion_main!(benches);
