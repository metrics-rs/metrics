use criterion::{criterion_group, criterion_main, Criterion};

use metrics::{NameParts, SharedString};

fn key_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("key");
    group.bench_function("name_parts/to_string", |b| {
        static NAME_PARTS: [SharedString; 2] = [
            SharedString::const_str("part1"),
            SharedString::const_str("part2"),
        ];
        let name = NameParts::from_static_names(&NAME_PARTS);
        b.iter(|| name.to_string())
    });
    group.bench_function("name_parts/Display::to_string", |b| {
        static NAME_PARTS: [SharedString; 2] = [
            SharedString::const_str("part1"),
            SharedString::const_str("part2"),
        ];
        let name = NameParts::from_static_names(&NAME_PARTS);
        b.iter(|| std::fmt::Display::to_string(&name))
    });
    group.finish();
}

criterion_group!(benches, key_benchmark);
criterion_main!(benches);
