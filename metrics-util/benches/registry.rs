use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use metrics::{Key, Label};
use metrics_util::registry::Registry;

fn registry_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("registry");
    group.bench_function("cached op (basic)", |b| {
        let registry = Registry::atomic();
        static KEY_NAME: &'static str = "simple_key";
        static KEY_DATA: Key = Key::from_static_name(&KEY_NAME);

        b.iter(|| registry.get_or_create_counter(&KEY_DATA, |_| ()))
    });
    group.bench_function("cached op (labels)", |b| {
        let registry = Registry::atomic();
        static KEY_NAME: &'static str = "simple_key";
        static KEY_LABELS: [Label; 1] = [Label::from_static_parts("type", "http")];
        static KEY_DATA: Key = Key::from_static_parts(&KEY_NAME, &KEY_LABELS);

        b.iter(|| registry.get_or_create_counter(&KEY_DATA, |_| ()))
    });
    group.bench_function("uncached op (basic)", |b| {
        b.iter_batched_ref(
            || Registry::atomic(),
            |registry| {
                let key = "simple_key".into();
                registry.get_or_create_counter(&key, |_| ())
            },
            BatchSize::SmallInput,
        )
    });
    group.bench_function("uncached op (labels)", |b| {
        b.iter_batched_ref(
            || Registry::atomic(),
            |registry| {
                let labels = vec![Label::new("type", "http")];
                let key = ("simple_key", labels).into();
                registry.get_or_create_counter(&key, |_| ())
            },
            BatchSize::SmallInput,
        )
    });
    group.bench_function("creation overhead", |b| {
        b.iter_batched(|| (), |_| Registry::atomic(), BatchSize::NumIterations(1))
    });
    group.bench_function("const key overhead (basic)", |b| {
        b.iter(|| {
            static KEY_NAME: &'static str = "simple_key";
            Key::from_static_name(&KEY_NAME)
        })
    });
    group.bench_function("const key data overhead (labels)", |b| {
        b.iter(|| {
            static KEY_NAME: &'static str = "simple_key";
            static LABELS: [Label; 1] = [Label::from_static_parts("type", "http")];
            Key::from_static_parts(&KEY_NAME, &LABELS)
        })
    });
    group.bench_function("owned key overhead (basic)", |b| b.iter(|| Key::from_name("simple_key")));
    group.bench_function("owned key overhead (labels)", |b| {
        b.iter(|| {
            let key = "simple_key";
            let labels = vec![Label::new("type", "http")];
            Key::from_parts(key, labels)
        })
    });
    group.finish();
}

criterion_group!(benches, registry_benchmark);
criterion_main!(benches);
