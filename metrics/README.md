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

# caveat emptor

This crate is currently materializing! We are in the process of switching over [hotmic](https://github.com/nuclearfurnace/hotmic) to `metrics` after successfully acquiring ownership of the `metrics` crate on crates.io!

We apologize for the README/documentation that will reference things that don't exist yet until the switchover is complete.  Thank you for your understanding!

## general features
- Provides counter, gauge, and histogram support.
- Access to ultra-high-speed timing facilities out-of-the-box with [quanta](https://github.com/nuclearfurnace/quanta).
- Scoped metrics for effortless nesting.
- Speed and API ergonomics allow for usage in both synchronous and asynchronous contexts.
- Based on `metrics-core` for bring-your-own-collector/bring-your-own-exporter flexibility!

## performance

High. Tens of millions of metrics per second with metric ingest times at sub-200ns p99 on modern systems.
