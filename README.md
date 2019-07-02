# metrics

[![conduct-badge][]][conduct] [![downloads-badge][] ![release-badge][]][crate] [![docs-badge][]][docs] [![license-badge][]](#license)

[conduct-badge]: https://img.shields.io/badge/%E2%9D%A4-code%20of%20conduct-blue.svg
[downloads-badge]: https://img.shields.io/crates/d/metrics.svg
[release-badge]: https://img.shields.io/crates/v/metrics.svg
[license-badge]: https://img.shields.io/crates/l/metrics.svg
[docs-badge]: https://docs.rs/metrics/badge.svg
[conduct]: https://github.com/metrics-rs/metrics/blob/master/CODE_OF_CONDUCT.md
[crate]: https://crates.io/crates/metrics
[docs]: https://docs.rs/metrics

__metrics__ is a high-quality, batteries-included metrics library for Rust.

## code of conduct

**NOTE**: All conversations and contributions to this project shall adhere to the [Code of Conduct][conduct].

# what's it all about?

Running applications in production can be hard when you don't have insight into what the application is doing.  We're lucky to have so many good system monitoring programs and services to show us how are servers are performing, but we still have to do the work of instrumenting our applications to gain deep insight into their behavior and performance.

`metrics` makes it easy to instrument your application to provide real-time insight into what's happening.  It provides a straight-forward interface for you to collect metrics at different points, and a flexible approach to exporting those metrics in a way that meets your needs.

Some of the most common scenarios for collecting metrics from an application:
- see how many times a codepath was hit
- track the time it takes for a piece of code to execute
- expose internal counters and values in a standardized way

The number of reasons why you'd want to collect metrics is too large to list out here, and some applications emit metrics that have nothing to do with the application performance itself!  Ultimately, `metrics` strives to simply provide support for the most basic types of metrics so that you can spend more time focusing on the data you'd like to collect and less time on how you're going to accomplish that.

## high-level technical features
- Supports the three most common metric types: counters, gauges, and histograms.
- Based on `metrics-core` for composability at the exporter level.
- Access to ultra-high-speed timing facilities out-of-the-box with [quanta](https://github.com/nuclearfurnace/quanta).
- Scoped metrics for effortless nesting.
- Bundled with Prometheus pull endpoint capabilities by default.

## performance

High. `metrics` is fast enough that you'll barely notice the overhead.

There is a `benchmark` example in the crate that can be run to see the type of performance achievable on your system.  A 2015 MacBook Pro (4c/8t, 2.1GHz) can push over 5 million samples per second from a single thread.
