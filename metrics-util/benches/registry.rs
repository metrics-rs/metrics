#[macro_use]
extern crate criterion;

use criterion::{BatchSize, Benchmark, Criterion};
use metrics::{Key, KeyData, Label, OnceKeyData};
use metrics_util::Registry;

fn registry_benchmark(c: &mut Criterion) {
    c.bench(
        "registry",
        Benchmark::new("cached op (basic)", |b| {
            let registry: Registry<Key, ()> = Registry::new();
            static KEY_DATA: OnceKeyData = OnceKeyData::new();

            b.iter(|| {
                let key = Key::Borrowed(KEY_DATA.get_or_init(|| KeyData::from_name("simple_key")));
                registry.op(key, |_| (), || ())
            })
        })
        .with_function("cached op (labels)", |b| {
            let registry: Registry<Key, ()> = Registry::new();
            static KEY_DATA: OnceKeyData = OnceKeyData::new();

            b.iter(|| {
                let key = Key::Borrowed(KEY_DATA.get_or_init(|| {
                    let labels = vec![Label::new("type", "http")];
                    KeyData::from_name_and_labels("simple_key", labels)
                }));
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
                KeyData::from_name_and_labels(key, labels)
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
                Key::Owned(KeyData::from_name_and_labels(key, labels))
            })
        })
        .with_function("cached key overhead (basic)", |b| {
            static KEY_DATA: OnceKeyData = OnceKeyData::new();
            b.iter(|| {
                let key_data = KEY_DATA.get_or_init(|| {
                    let key = "simple_key";
                    KeyData::from_name(key)
                });
                Key::Borrowed(key_data)
            })
        })
        .with_function("cached key overhead (labels)", |b| {
            static KEY_DATA: OnceKeyData = OnceKeyData::new();
            b.iter(|| {
                let key_data = KEY_DATA.get_or_init(|| {
                    let key = "simple_key";
                    let labels = vec![Label::new("type", "http")];
                    KeyData::from_name_and_labels(key, labels)
                });
                Key::Borrowed(key_data)
            })
        }),
    );
}

criterion_group!(benches, registry_benchmark);
criterion_main!(benches);
