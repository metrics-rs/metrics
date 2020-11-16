use criterion::{criterion_group, criterion_main, BatchSize, Benchmark, Criterion};
use metrics::{Key, KeyData, Label, SharedString};
use metrics_util::Registry;

fn registry_benchmark(c: &mut Criterion) {
    c.bench(
        "registry",
        Benchmark::new("cached op (basic)", |b| {
            let registry: Registry<Key, ()> = Registry::new();
            static KEY_NAME: [SharedString; 1] = [SharedString::const_str("simple_key")];
            static KEY_DATA: KeyData = KeyData::from_static_name(&KEY_NAME);

            b.iter(|| {
                let key = Key::Borrowed(&KEY_DATA);
                registry.op(key, |_| (), || ())
            })
        })
        .with_function("cached op (labels)", |b| {
            let registry: Registry<Key, ()> = Registry::new();
            static KEY_NAME: [SharedString; 1] = [SharedString::const_str("simple_key")];
            static KEY_LABELS: [Label; 1] = [Label::from_static_parts("type", "http")];
            static KEY_DATA: KeyData = KeyData::from_static_parts(&KEY_NAME, &KEY_LABELS);

            b.iter(|| {
                let key = Key::Borrowed(&KEY_DATA);
                registry.op(key, |_| (), || ())
            })
        })
        .with_function("uncached op (basic)", |b| {
            b.iter_batched_ref(
                || Registry::<Key, ()>::new(),
                |registry| {
                    let key = Key::Owned("simple_key".into());
                    registry.op(key, |_| (), || ())
                },
                BatchSize::SmallInput,
            )
        })
        .with_function("uncached op (labels)", |b| {
            b.iter_batched_ref(
                || Registry::<Key, ()>::new(),
                |registry| {
                    let labels = vec![Label::new("type", "http")];
                    let key = Key::Owned(("simple_key", labels).into());
                    registry.op(key, |_| (), || ())
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
        .with_function("key data overhead (basic)", |b| {
            b.iter(|| {
                let key = "simple_key";
                KeyData::from_name(key)
            })
        })
        .with_function("key data overhead (labels)", |b| {
            b.iter(|| {
                let key = "simple_key";
                let labels = vec![Label::new("type", "http")];
                KeyData::from_parts(key, labels)
            })
        })
        .with_function("const key data overhead (basic)", |b| {
            b.iter(|| {
                static KEY_NAME: [SharedString; 1] = [SharedString::const_str("simple_key")];
                KeyData::from_static_name(&KEY_NAME)
            })
        })
        .with_function("const key data overhead (labels)", |b| {
            b.iter(|| {
                static KEY_NAME: [SharedString; 1] = [SharedString::const_str("simple_key")];
                static LABELS: [Label; 1] = [Label::from_static_parts("type", "http")];
                KeyData::from_static_parts(&KEY_NAME, &LABELS)
            })
        })
        .with_function("owned key overhead (basic)", |b| {
            b.iter(|| {
                let key = "simple_key";
                Key::Owned(KeyData::from_name(key))
            })
        })
        .with_function("owned key overhead (labels)", |b| {
            b.iter(|| {
                let key = "simple_key";
                let labels = vec![Label::new("type", "http")];
                Key::Owned(KeyData::from_parts(key, labels))
            })
        })
        .with_function("cached key overhead (basic)", |b| {
            static KEY_NAME: [SharedString; 1] = [SharedString::const_str("simple_key")];
            static KEY_DATA: KeyData = KeyData::from_static_name(&KEY_NAME);
            b.iter(|| Key::Borrowed(&KEY_DATA))
        })
        .with_function("cached key overhead (labels)", |b| {
            static KEY_NAME: [SharedString; 1] = [SharedString::const_str("simple_key")];
            static KEY_LABELS: [Label; 1] = [Label::from_static_parts("type", "http")];
            static KEY_DATA: KeyData = KeyData::from_static_parts(&KEY_NAME, &KEY_LABELS);
            b.iter(|| Key::Borrowed(&KEY_DATA))
        }),
    );
}

criterion_group!(benches, registry_benchmark);
criterion_main!(benches);
