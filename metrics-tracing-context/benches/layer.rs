use criterion::{criterion_group, criterion_main, Criterion};
use metrics::{Key, Label, NoopRecorder, Recorder};
use metrics_tracing_context::{MetricsLayer, TracingContextLayer};
use metrics_util::layers::Layer;
use tracing::{
    dispatcher::{with_default, Dispatch},
    span, Level,
};
use tracing_subscriber::{layer::SubscriberExt, Registry};

fn layer_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("layer");
    group.bench_function("base case", |b| {
        let recorder = NoopRecorder;
        static KEY_NAME: &str = "key";
        static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
        static KEY_DATA: Key = Key::from_static_parts(KEY_NAME, &KEY_LABELS);
        static METADATA: metrics::Metadata =
            metrics::Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));

        b.iter(|| {
            let _ = recorder.register_counter(&KEY_DATA, &METADATA);
        })
    });
    group.bench_function("no integration", |b| {
        let subscriber = Registry::default();
        let dispatcher = Dispatch::new(subscriber);
        with_default(&dispatcher, || {
            let user = "ferris";
            let email = "ferris@rust-lang.org";
            let span = span!(Level::TRACE, "login", user, user.email = email);
            let _guard = span.enter();

            let recorder = NoopRecorder;
            static KEY_NAME: &str = "key";
            static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
            static KEY_DATA: Key = Key::from_static_parts(KEY_NAME, &KEY_LABELS);
            static METADATA: metrics::Metadata =
                metrics::Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));

            b.iter(|| {
                let _ = recorder.register_counter(&KEY_DATA, &METADATA);
            })
        })
    });
    group.bench_function("tracing layer only", |b| {
        let subscriber = Registry::default().with(MetricsLayer::new());
        let dispatcher = Dispatch::new(subscriber);
        with_default(&dispatcher, || {
            let user = "ferris";
            let email = "ferris@rust-lang.org";
            let span = span!(Level::TRACE, "login", user, user.email = email);
            let _guard = span.enter();

            let recorder = NoopRecorder;
            static KEY_NAME: &str = "key";
            static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
            static KEY_DATA: Key = Key::from_static_parts(KEY_NAME, &KEY_LABELS);
            static METADATA: metrics::Metadata =
                metrics::Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));

            b.iter(|| {
                let _ = recorder.register_counter(&KEY_DATA, &METADATA);
            })
        })
    });
    group.bench_function("metrics layer only", |b| {
        let subscriber = Registry::default();
        let dispatcher = Dispatch::new(subscriber);
        with_default(&dispatcher, || {
            let user = "ferris";
            let email = "ferris@rust-lang.org";
            let span = span!(Level::TRACE, "login", user, user.email = email);
            let _guard = span.enter();

            let tracing_layer = TracingContextLayer::all();
            let recorder = tracing_layer.layer(NoopRecorder);
            static KEY_NAME: &str = "key";
            static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
            static KEY_DATA: Key = Key::from_static_parts(KEY_NAME, &KEY_LABELS);
            static METADATA: metrics::Metadata =
                metrics::Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));

            b.iter(|| {
                let _ = recorder.register_counter(&KEY_DATA, &METADATA);
            })
        })
    });
    group.bench_function("full integration", |b| {
        let subscriber = Registry::default().with(MetricsLayer::new());
        let dispatcher = Dispatch::new(subscriber);
        with_default(&dispatcher, || {
            let user = "ferris";
            let email = "ferris@rust-lang.org";
            let span = span!(Level::TRACE, "login", user, user.email = email);
            let _guard = span.enter();

            let tracing_layer = TracingContextLayer::all();
            let recorder = tracing_layer.layer(NoopRecorder);
            static KEY_NAME: &str = "key";
            static KEY_LABELS: [Label; 1] = [Label::from_static_parts("foo", "bar")];
            static KEY_DATA: Key = Key::from_static_parts(KEY_NAME, &KEY_LABELS);
            static METADATA: metrics::Metadata =
                metrics::Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));

            b.iter(|| {
                let _ = recorder.register_counter(&KEY_DATA, &METADATA);
            })
        })
    });
    group.finish();
}

criterion_group!(benches, layer_benchmark);
criterion_main!(benches);
