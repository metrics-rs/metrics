# metrics

[![conduct-badge][]][conduct] [![downloads-badge][] ![release-badge][]][crate] [![docs-badge][]][docs] [![license-badge][]](#license)

[conduct-badge]: https://img.shields.io/badge/%E2%9D%A4-code%20of%20conduct-blue.svg
[downloads-badge]: https://img.shields.io/crates/d/metrics-runtime.svg
[release-badge]: https://img.shields.io/crates/v/metrics-runtime.svg
[license-badge]: https://img.shields.io/crates/l/metrics-runtime.svg
[docs-badge]: https://docs.rs/metrics-runtime/badge.svg
[conduct]: https://github.com/metrics-rs/metrics/blob/master/CODE_OF_CONDUCT.md
[crate]: https://crates.io/crates/metrics-runtime
[docs]: https://docs.rs/metrics-runtime

__metrics__ is a batteries-included metrics library.

## code of conduct

**NOTE**: All conversations and contributions to this project shall adhere to the [Code of Conduct][conduct].

# what's it all about?

`metrics-runtime` is the high-quality, batteries-included reference metrics runtime for the Metrics project.

This crate serves to provide support for all of the goals espoused by the project as a whole: a runtime that can be used with `metrics`, support for interoperating with `metrics-core` compatible observers and exporters.  On top of that, it provides a deliberately designed API meant to help you quickly and easily instrument your application.

As operators of systems at scale, we've attempted to distill this library down to the core features necessary to successfully instrument an application and ensure that you succeed at providing observability into your production systems.

## high-level technical features
- Supports the three most common metric types: counters, gauges, and histograms.
- Based on `metrics-core` for composability at the observer/exporter level.
- Access to ultra-high-speed timing facilities out-of-the-box with [quanta](https://github.com/nuclearfurnace/quanta).
- Scoped and labeled metrics for rich dimensionality.
- Bundled with a number of useful observers/exporters: export your metrics with ease.

## performance

Even as a reference runtime, `metrics-runtime` still has extremely impressive performance. On modern cloud systems, you'll be able to ingest millions of samples per second per core with p99 latencies in the low hundreds of nanoseconds.  While `metrics-runtime` will not be low-enough overhead for every use case, it will meet or exceed the performance of other metrics libraries in Rust and in turn providing you wih fast and predictably low-overhead measurements under production workloads.

There are a few example benchmark programs in the crate that simulate basic workloads.  These programs specifically do not attempt to fully simulate a production workload, in terms of number of metrics, frequency of ingestion, or dimensionality.  They are brute force benchmarks designed to showcase throughput and latency for varied concurrency profiles under high write contention.
