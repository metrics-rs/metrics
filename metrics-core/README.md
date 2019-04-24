# metrics-core

[![conduct-badge][]][conduct] [![downloads-badge][] ![release-badge][]][crate] [![docs-badge][]][docs] [![license-badge][]](#license)

[conduct-badge]: https://img.shields.io/badge/%E2%9D%A4-code%20of%20conduct-blue.svg
[downloads-badge]: https://img.shields.io/crates/d/metrics-core.svg
[release-badge]: https://img.shields.io/crates/v/metrics-core.svg
[license-badge]: https://img.shields.io/crates/l/metrics-core.svg
[docs-badge]: https://docs.rs/metrics-core/badge.svg
[conduct]: https://github.com/metrics-rs/metrics-core/blob/master/CODE_OF_CONDUCT.md
[crate]: https://crates.io/crates/metrics-core
[docs]: https://docs.rs/metrics-core

__metrics-core__ defines foundational traits for interoperable metrics libraries in Rust.

## code of conduct

**NOTE**: All conversations and contributions to this project shall adhere to the [Code of Conduct][conduct].

## mandate / goals

This crate acts as the minimum viable trait for metrics libraries, and consumers of that data, for interoperating with each other.

If your library allows users to collect metrics, it should support metrics-core to allow for flexibility in output targets.  If your library provides support for a target metrics backend, it should support metrics-core so that it can be easily plugged into applications using a supported metrics library.
