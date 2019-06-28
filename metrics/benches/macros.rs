#[macro_use]
extern crate criterion;

#[macro_use]
extern crate metrics;

use criterion::{Benchmark, Criterion};

fn macro_benchmark(c: &mut Criterion) {
    c.bench(
        "counter",
        Benchmark::new("no labels", |b| b.iter(|| {
            counter!("counter_bench", 42);
        }))
        .with_function("with labels", |b| b.iter(|| {
            counter!("counter_bench", 42, "request" => "http", "svc" => "admin");
        }))
    );
}

criterion_group!(benches, macro_benchmark);
criterion_main!(benches);
