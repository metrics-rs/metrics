#[macro_use]
extern crate criterion;

use criterion::{BatchSize, Benchmark, Criterion};
use metrics::{Key, Label};
use metrics_util::Registry;

fn registry_benchmark(c: &mut Criterion) {
    c.bench(
        "registry",
        Benchmark::new("cached get/create (basic)", |b| {
            let registry: Registry<Key, ()> = Registry::new();

            b.iter(|| {
                let key = "simple_key".into();
                let _ = registry.get_or_create_identifier(key, |_| ());
            })
        })
        .with_function("cached get/create (labels)", |b| {
            let registry: Registry<Key, ()> = Registry::new();

            b.iter(|| {
                let labels = vec![Label::from_static("type", "http")];
                let key = ("simple_key", labels).into();
                let _ = registry.get_or_create_identifier(key, |_| ());
            })
        })
        .with_function("uncached get/create (basic)", |b| {
            b.iter_batched_ref(
                || Registry::<Key, ()>::new(),
                |registry| {
                    let key = "simple_key".into();
                    let _ = registry.get_or_create_identifier(key, |_| ());
                },
                BatchSize::SmallInput,
            )
        })
        .with_function("uncached get/create (labels)", |b| {
            b.iter_batched_ref(
                || Registry::<Key, ()>::new(),
                |registry| {
                    let labels = vec![Label::from_static("type", "http")];
                    let key = ("simple_key", labels).into();
                    let _ = registry.get_or_create_identifier(key, |_| ());
                },
                BatchSize::SmallInput,
            )
        })
        .with_function("with handle", |b| {
            let registry = Registry::<Key, ()>::new();
            let id = registry.get_or_create_identifier("foo".into(), |_| ());

            b.iter(|| registry.with_handle(id, |_| {}))
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
                let _: Key = key.into();
            })
        })
        .with_function("key overhead (labels)", |b| {
            b.iter(|| {
                let key = "simple_key";
                let labels = vec![Label::from_static("type", "http")];
                let _: Key = (key, labels).into();
            })
        }),
    );
}

criterion_group!(benches, registry_benchmark);
criterion_main!(benches);
