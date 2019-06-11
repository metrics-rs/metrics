# metrics-facade

[![conduct-badge][]][conduct] [![downloads-badge][] ![release-badge][]][crate] [![docs-badge][]][docs] [![license-badge][]](#license)

[conduct-badge]: https://img.shields.io/badge/%E2%9D%A4-code%20of%20conduct-blue.svg
[downloads-badge]: https://img.shields.io/crates/d/metrics-facade.svg
[release-badge]: https://img.shields.io/crates/v/metrics-facade.svg
[license-badge]: https://img.shields.io/crates/l/metrics-facade.svg
[docs-badge]: https://docs.rs/metrics-facade/badge.svg
[conduct]: https://github.com/metrics-rs/metrics/blob/master/CODE_OF_CONDUCT.md
[crate]: https://crates.io/crates/metrics-facade
[docs]: https://docs.rs/metrics-facade

__metrics-facade__ is a lightweight metrics facade.

## code of conduct

**NOTE**: All conversations and contributions to this project shall adhere to the [Code of Conduct][conduct].

# what's it all about?

__metrics-facade__ provides macros, similar to the [`log`](https://docs.rs/log) crate, that let library and executable authors instrument their code by collecting metrics -- incrementing counters, gauges, and histograms -- about their code, deferring the collecting and export of these metrics to whatever the installed metrics library is.
