#[macro_use]
extern crate criterion;

#[macro_use]
extern crate lazy_static;

use criterion::{Benchmark, Criterion, Throughput};
use metrics_util::StreamingIntegers;
use rand::{distributions::Distribution, rngs::SmallRng, SeedableRng};
use rand_distr::Gamma;
use std::time::Duration;

lazy_static! {
    static ref NORMAL_SMALL: Vec<u64> = get_gamma_distribution(100, Duration::from_millis(200));
    static ref NORMAL_MEDIUM: Vec<u64> = get_gamma_distribution(10000, Duration::from_millis(200));
    static ref NORMAL_LARGE: Vec<u64> = get_gamma_distribution(1000000, Duration::from_millis(200));
    static ref LINEAR_SMALL: Vec<u64> = get_linear_distribution(100);
    static ref LINEAR_MEDIUM: Vec<u64> = get_linear_distribution(10000);
    static ref LINEAR_LARGE: Vec<u64> = get_linear_distribution(1000000);
}

fn get_gamma_distribution(len: usize, upper_bound: Duration) -> Vec<u64> {
    // Start with a seeded RNG so that we predictably regenerate our data.
    let mut rng = SmallRng::seed_from_u64(len as u64);

    // This Gamma distribution gets us pretty close to a typical web server response time
    // distribution where there's a big peak down low, and a long tail that drops off sharply.
    let gamma = Gamma::new(1.75, 1.0).expect("failed to create gamma distribution");

    // Scale all the values by 22 million to simulate a lower bound of 22ms (but in
    // nanoseconds) for all generated values.
    gamma
        .sample_iter(&mut rng)
        .map(|n| n * upper_bound.as_nanos() as f64)
        .map(|n| n as u64)
        .take(len)
        .collect::<Vec<u64>>()
}

fn get_linear_distribution(len: usize) -> Vec<u64> {
    let mut values = Vec::new();
    for i in 0..len as u64 {
        values.push(i);
    }
    values
}

macro_rules! define_basic_benches {
    ($c:ident, $name:expr, $input:ident) => {
        $c.bench(
            $name,
            Benchmark::new("compress", |b| {
                b.iter_with_large_drop(|| {
                    let mut si = StreamingIntegers::new();
                    si.compress(&$input);
                    si
                })
            })
            .with_function("decompress", |b| {
                let mut si = StreamingIntegers::new();
                si.compress(&$input);

                b.iter_with_large_drop(move || si.decompress())
            })
            .with_function("decompress + sum", |b| {
                let mut si = StreamingIntegers::new();
                si.compress(&$input);

                b.iter_with_large_drop(move || {
                    let total: u64 = si.decompress().iter().sum();
                    total
                })
            })
            .with_function("decompress_with + sum", |b| {
                let mut si = StreamingIntegers::new();
                si.compress(&$input);

                b.iter(move || {
                    let mut total = 0;
                    si.decompress_with(|batch| {
                        let batch_total: u64 = batch.iter().sum();
                        total += batch_total;
                    });
                });
            })
            .throughput(Throughput::Elements($input.len() as u64)),
        )
    };
}

fn streaming_integer_benchmark(c: &mut Criterion) {
    define_basic_benches!(c, "normal small", NORMAL_SMALL);
    define_basic_benches!(c, "normal medium", NORMAL_MEDIUM);
    define_basic_benches!(c, "normal large", NORMAL_LARGE);
    define_basic_benches!(c, "linear small", LINEAR_SMALL);
    define_basic_benches!(c, "linear medium", LINEAR_MEDIUM);
    define_basic_benches!(c, "linear large", LINEAR_LARGE);
}

criterion_group!(benches, streaming_integer_benchmark);
criterion_main!(benches);
