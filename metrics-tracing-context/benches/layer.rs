use criterion::{criterion_group, criterion_main, Criterion};
use metrics::{Key, KeyData, Label, NoopRecorder, Recorder, SharedString};
use metrics_tracing_context::{MetricsLayer, TracingContextLayer};
use metrics_util::layers::Layer;
use tracing::{
    dispatcher::{with_default, Dispatch},
    span, Level,
};
use tracing_subscriber::{layer::SubscriberExt, Registry};

fn layer_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("layer");
    group.bench_function("all/enhance_key", |b| {
        let subscriber = Registry::default().with(MetricsLayer::new());
        let dispatcher = Dispatch::new(subscriber);
        with_default(&dispatcher, || {
            let user = "ferris";
            let email = "ferris@rust-lang.org";
            let span = span!(Level::TRACE, "login", user, user.email = email);
            let _guard = span.enter();

            let tracing_layer = TracingContextLayer::all();
            let recorder = tracing_layer.layer(NoopRecorder);
            static KEY_NAME: [SharedString; 1] = [SharedString::const_str("key")];
            static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
            static KEY_DATA: KeyData = KeyData::from_static_parts(&KEY_NAME, &KEY_LABELS);

            b.iter(|| {
                recorder.increment_counter(Key::Borrowed(&KEY_DATA), 1);
            })
        })
    });
    group.bench_function("noop recorder overhead (increment_counter)", |b| {
        let recorder = NoopRecorder;
        static KEY_NAME: [SharedString; 1] = [SharedString::const_str("key")];
        static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
        static KEY_DATA: KeyData = KeyData::from_static_parts(&KEY_NAME, &KEY_LABELS);

        b.iter(|| {
            recorder.increment_counter(Key::Borrowed(&KEY_DATA), 1);
        })
    });
    group.finish();
}

criterion_group!(benches, layer_benchmark);
criterion_main!(benches);
