use criterion::{criterion_group, criterion_main, Criterion};
use metrics::SharedString;

fn shared_descriptions_benchmark(c: &mut Criterion) {
    // Compare cloning a dynamic description into thirty-two consumers. Preparing retained
    // storage is outside the measured loop, as it is done once before distributing metadata.
    let owned = SharedString::from("Example description ".repeat(16));
    let shared = owned.to_retained();
    let mut group = c.benchmark_group("shared_descriptions/clone_thirty_two");

    group.bench_function("owned", |b| {
        b.iter(|| (0..32).map(|_| owned.clone()).collect::<Vec<_>>());
    });
    group.bench_function("shared", |b| {
        b.iter(|| (0..32).map(|_| shared.clone()).collect::<Vec<_>>());
    });
    group.finish();
}

criterion_group!(benches, shared_descriptions_benchmark);
criterion_main!(benches);
