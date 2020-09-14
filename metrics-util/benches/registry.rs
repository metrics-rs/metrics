#[macro_use]
extern crate criterion;

use criterion::{BatchSize, Benchmark, Criterion};
use metrics::{Key, KeyData, Label, OnceKeyData};
use metrics_util::Registry;

fn registry_benchmark(c: &mut Criterion) {
    c.bench(
        "registry",
        Benchmark::new("cached get/create (basic)", |b| {
            let registry: Registry<Key, ()> = Registry::new();
            static KEY_DATA: OnceKeyData = OnceKeyData::new();

            b.iter(|| {
                let key = Key::Borrowed(KEY_DATA.get_or_init(|| KeyData::from_name("simple_key")));
                let _ = registry.get_or_create_identifier(key, |_| ());
            })
        })
        .with_function("cached get/create (labels)", |b| {
            let registry: Registry<Key, ()> = Registry::new();
            static KEY_DATA: OnceKeyData = OnceKeyData::new();

            b.iter(|| {
                let key = Key::Borrowed(KEY_DATA.get_or_init(|| {
                    let labels = vec![Label::new("type", "http")];
                    KeyData::from_name_and_labels("simple_key", labels)
                }));
                let _ = registry.get_or_create_identifier(key, |_| ());
            })
        })
        .with_function("uncached get/create (basic)", |b| {
            b.iter_batched_ref(
                || Registry::<Key, ()>::new(),
                |registry| {
                    let key = Key::Owned("simple_key".into());
                    let _ = registry.get_or_create_identifier(key, |_| ());
                },
                BatchSize::SmallInput,
            )
        })
        .with_function("uncached get/create (labels)", |b| {
            b.iter_batched_ref(
                || Registry::<Key, ()>::new(),
                |registry| {
                    let labels = vec![Label::new("type", "http")];
                    let key = Key::Owned(("simple_key", labels).into());
                    let _ = registry.get_or_create_identifier(key, |_| ());
                },
                BatchSize::SmallInput,
            )
        })
        .with_function("with handle", |b| {
            let registry = Registry::<Key, ()>::new();
            let id = registry.get_or_create_identifier(KeyData::from_name("foo").into(), |_| ());

            b.iter(|| registry.with_handle(id.clone(), |_| {}))
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
                let _: KeyData = KeyData::from_name(key);
            })
        })
        .with_function("key overhead (labels)", |b| {
            b.iter(|| {
                let key = "simple_key";
                let labels = vec![Label::new("type", "http")];
                let _: KeyData = KeyData::from_name_and_labels(key, labels);
            })
        }),
    );
}

criterion_group!(benches, registry_benchmark);
criterion_main!(benches);
