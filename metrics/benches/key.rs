use criterion::{criterion_group, criterion_main, Benchmark, Criterion};

use metrics::{NameParts, SharedString};

fn key_benchmark(c: &mut Criterion) {
    c.bench(
        "key",
        Benchmark::new("name_parts/to_string", |b| {
            static NAME_PARTS: [SharedString; 2] = [
                SharedString::const_str("part1"),
                SharedString::const_str("part2"),
            ];
            let name = NameParts::from_static_names(&NAME_PARTS);
            b.iter(|| name.to_string())
        })
        .with_function("name_parts/Display::to_string", |b| {
            static NAME_PARTS: [SharedString; 2] = [
                SharedString::const_str("part1"),
                SharedString::const_str("part2"),
            ];
            let name = NameParts::from_static_names(&NAME_PARTS);
            b.iter(|| std::fmt::Display::to_string(&name))
        }),
    );
}

criterion_group!(benches, key_benchmark);
criterion_main!(benches);
