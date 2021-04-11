use criterion::{criterion_group, criterion_main, BatchSize, Benchmark, Criterion};
use metrics::{Key, Label, SharedString};
use metrics_util::{MetricKind, Registry};

fn registry_benchmark(c: &mut Criterion) {
    c.bench(
        "registry",
        Benchmark::new("cached op (basic)", |b| {
            let registry: Registry<Key, ()> = Registry::new();
            static KEY_NAME: [SharedString; 1] = [SharedString::const_str("simple_key")];
            static KEY_DATA: Key = Key::from_static_name(&KEY_NAME);

            b.iter(|| {
                registry.op(MetricKind::Counter, &KEY_DATA, |_| (), || ())
            })
        })
        .with_function("cached op (labels)", |b| {
            let registry: Registry<Key, ()> = Registry::new();
            static KEY_NAME: [SharedString; 1] = [SharedString::const_str("simple_key")];
            static KEY_LABELS: [Label; 1] = [Label::from_static_parts("type", "http")];
            static KEY_DATA: Key = Key::from_static_parts(&KEY_NAME, &KEY_LABELS);

            b.iter(|| {
                registry.op(MetricKind::Counter, &KEY_DATA, |_| (), || ())
            })
        })
        .with_function("uncached op (basic)", |b| {
            b.iter_batched_ref(
                || Registry::<Key, ()>::new(),
                |registry| {
                    let key = "simple_key".into();
                    registry.op(MetricKind::Counter, &key, |_| (), || ())
                },
                BatchSize::SmallInput,
            )
        })
        .with_function("uncached op (labels)", |b| {
            b.iter_batched_ref(
                || Registry::<Key, ()>::new(),
                |registry| {
                    let labels = vec![Label::new("type", "http")];
                    let key = ("simple_key", labels).into();
                    registry.op(MetricKind::Counter, &key, |_| (), || ())
                },
                BatchSize::SmallInput,
            )
        })
        .with_function("registry overhead", |b| {
            b.iter_batched(
                || (),
                |_| Registry::<(), ()>::new(),
                BatchSize::NumIterations(1),
            )
        })
        .with_function("key overhead (basic)", |b| {
            b.iter(|| {
                let key = "simple_key";
                Key::from_name(key)
            })
        })
        .with_function("key overhead (labels)", |b| {
            b.iter(|| {
                let key = "simple_key";
                let labels = vec![Label::new("type", "http")];
                Key::from_parts(key, labels)
            })
        })
        .with_function("const key overhead (basic)", |b| {
            b.iter(|| {
                static KEY_NAME: [SharedString; 1] = [SharedString::const_str("simple_key")];
                Key::from_static_name(&KEY_NAME)
            })
        })
        .with_function("const key data overhead (labels)", |b| {
            b.iter(|| {
                static KEY_NAME: [SharedString; 1] = [SharedString::const_str("simple_key")];
                static LABELS: [Label; 1] = [Label::from_static_parts("type", "http")];
                Key::from_static_parts(&KEY_NAME, &LABELS)
            })
        })
        .with_function("owned key overhead (basic)", |b| {
            b.iter(|| {
                Key::from_name("simple_key")
            })
        })
        .with_function("owned key overhead (labels)", |b| {
            b.iter(|| {
                let key = "simple_key";
                let labels = vec![Label::new("type", "http")];
                Key::from_parts(key, labels)
            })
        }),
    );
}

criterion_group!(benches, registry_benchmark);
criterion_main!(benches);
