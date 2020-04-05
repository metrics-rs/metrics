#[macro_use]
extern crate criterion;

use criterion::{Criterion, Benchmark, BatchSize};
use metrics::{Key, Label};
use metrics_util::Registry;

fn registry_benchmark(c: &mut Criterion) {
    c.bench(
        "registry",
        Benchmark::new("cached get/create (basic)", |b| {
            let registry = Registry::new();

            b.iter(|| {
                let key = "simple_key".into();
                let _ = registry.get_or_create_identifier(key, ());
            })
        })
        .with_function("cached get/create (labels)", |b| {
            let registry = Registry::new();

            b.iter(|| {
                let labels = vec![Label::new("type", "http")];
                let key = ("simple_key", labels).into();
                let _ = registry.get_or_create_identifier(key, ());
            })
        })
        .with_function("uncached get/create (basic)", |b| {
            b.iter_batched_ref(|| Registry::new(), |registry| {
                let key = "simple_key".into();
                let _ = registry.get_or_create_identifier(key, ());
            }, BatchSize::SmallInput)
        })
        .with_function("uncached get/create (labels)", |b| {
            b.iter_batched_ref(|| Registry::new(), |registry| {
                let labels = vec![Label::new("type", "http")];
                let key = ("simple_key", labels).into();
                let _ = registry.get_or_create_identifier(key, ());
            }, BatchSize::SmallInput)
        })
        .with_function("get handle", |b| {
            let registry = Registry::new();
            let id = registry.get_or_create_identifier("foo".into(), ());

            b.iter(|| {
                let _handle = registry.get_handle(&id);
            })
        })
        .with_function("registry overhead", |b| {
            b.iter_batched(|| (), |_| Registry::<()>::new(), BatchSize::SmallInput)
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
                let labels = vec![Label::new("type", "http")];
                let _: Key = (key, labels).into();
            })
        }),
    );
}

criterion_group!(benches, registry_benchmark);
criterion_main!(benches);
