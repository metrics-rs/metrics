//! Measures how the cost of recording into a single histogram scales with the number of writer
//! threads.
//!
//! For each thread count, every thread records a fixed number of samples into the same histogram
//! of a `PrometheusBuilder::build_recorder()` recorder, and the measured time is how long it takes
//! for all of the threads to finish.  If recording did not contend between threads, that time
//! would stay flat as the thread count grows, and the reported throughput would grow linearly.
//!
//! The histogram is drained between iterations, as the exporter's upkeep task would, so that
//! samples do not accumulate without bound.
use std::time::{Duration, Instant};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use metrics::{Key, Level, Metadata, Recorder};
use metrics_exporter_prometheus::PrometheusBuilder;

const SAMPLES_PER_THREAD: usize = 100_000;

fn histogram_contention(c: &mut Criterion) {
    let recorder = PrometheusBuilder::new()
        .set_buckets(&[1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0])
        .expect("buckets are non-empty")
        .build_recorder();
    let handle = recorder.handle();
    let key = Key::from_name("bench_histogram");
    let metadata = Metadata::new(module_path!(), Level::INFO, None);
    let histogram = recorder.register_histogram(&key, &metadata);

    let mut group = c.benchmark_group("histogram_contention");
    for threads in [1, 2, 4, 8, 16] {
        group.throughput(Throughput::Elements((threads * SAMPLES_PER_THREAD) as u64));
        group.bench_with_input(BenchmarkId::from_parameter(threads), &threads, |b, &threads| {
            b.iter_custom(|iters| {
                let mut elapsed = Duration::ZERO;
                for _ in 0..iters {
                    let start = Instant::now();
                    std::thread::scope(|s| {
                        for _ in 0..threads {
                            s.spawn(|| {
                                for i in 0..SAMPLES_PER_THREAD {
                                    histogram.record((i % 100) as f64);
                                }
                            });
                        }
                    });
                    elapsed += start.elapsed();

                    handle.run_upkeep();
                }
                elapsed
            })
        });
    }
    group.finish();
}

criterion_group!(benches, histogram_contention);
criterion_main!(benches);
