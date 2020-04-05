#[macro_use]
extern crate criterion;

use criterion::{Benchmark, Criterion};
use metrics::{Key, Label};
use metrics_util::Registry;
use rand::{thread_rng, Rng};

fn registry_benchmark(c: &mut Criterion) {
    c.bench(
        "registry",
        Benchmark::new("cached (basic)", |b| {
            let registry = Registry::new();

            b.iter(|| {
                let key = "simple_key".into();
                let _ = registry.get_or_create_identifier(key, ());
            })
        })
        .with_function("cached (labels)", |b| {
            let registry = Registry::new();

            b.iter(|| {
                let labels = vec![Label::new("type", "http")];
                let key = ("simple_key", labels).into();
                let _ = registry.get_or_create_identifier(key, ());
            })
        })
        .with_function("uncached (basic)", |b| {
            let registry = Registry::new();

            b.iter(|| {
                let key = format!("simple_key_{}", thread_rng().gen::<usize>());
                let _ = registry.get_or_create_identifier(key.into(), ());
            })
        })
        .with_function("uncached (labels)", |b| {
            let registry = Registry::new();

            b.iter(|| {
                let labels = vec![Label::new("type", "http")];
                let key = format!("simple_key_{}", thread_rng().gen::<usize>());
                let _ = registry.get_or_create_identifier((key, labels).into(), ());
            })
        })
        .with_function("uncached offset (basic)", |b| {
            b.iter(|| {
                let key = format!("simple_key_{}", thread_rng().gen::<usize>());
                let _: Key = key.into();
            })
        })
        .with_function("uncached offset (labels)", |b| {
            b.iter(|| {
                let labels = vec![Label::new("type", "http")];
                let key = format!("simple_key_{}", thread_rng().gen::<usize>());
                let _: Key = (key, labels).into();
            })
        }),
    );
}

criterion_group!(benches, registry_benchmark);
criterion_main!(benches);
