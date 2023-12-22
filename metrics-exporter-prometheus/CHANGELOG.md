# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

## [0.12.2] - 2023-12-13

### Fixed

- Fixed overflow/underflow panic with time moving backwards ([#423](https://github.com/metrics-rs/metrics/pull/423))

## [0.12.1] - 2023-05-09

### Added

- Support for specifying a username/password for HTTP Basic Authentication when pushing to a Push
  Gateway. ([#366](https://github.com/metrics-rs/metrics/pull/366))

## [0.12.0] - 2023-04-16

### Changed

- Bump MSRV to 1.61.0.
- Switch to `metrics`-exposed version of `AtomicU64`.

## [0.11.0] - 2022-07-20

### Changed

- Aggregated summaries are now rolling, allowing oldering data points to expire and quantile values
  to reflect the recent past rather than the lifetime of a histogram.
  ([#306](https://github.com/metrics-rs/metrics/pull/306))

  They have a default width of three buckets, with each bucket being 20 seconds wide. This means
  only the last 60 seconds of a histogram -- in 20 second granularity -- will contribute to the
  quantiles emitted.

  We'll expose the ability to tune these values in the future.
- Switched to using `portable_atomic` for 64-bit atomics on more architectures.
  ([#313](https://github.com/metrics-rs/metrics/pull/313))


## [0.10.0] - 2022-05-30

### Fixed

- In some cases, metric names were being "sanitized" when they were already valid.
  ([#290](https://github.com/metrics-rs/metrics/pull/290), [#296](https://github.com/metrics-rs/metrics/pull/296))

## [0.9.0] - 2022-03-10

### Added

- New top-level module, `formatting`, which exposes many of the helper methods used to sanitize and
  render the actual Prometheus exposition format. ([#285](https://github.com/metrics-rs/metrics/pull/285))

## [0.8.0] - 2022-01-14

### Added

- New builder method, `PrometheusBuilder::install_recorder`, which builds and installs the
  recorder and returns a `PrometheusHandle` that can be used to interact with the recorder.

### Changed

- Updated various dependencies in order to properly scope dependencies to only the necessary feature
  flags, and thus optimize build times and reduce transitive dependencies.
- Updated to the new handle-based design of `metrics`.
- Renamed `tokio-exporter` feature flag to `http-listener`.
- Renamed `PrometheusBuilder::build` to `build_recorder`.
- Renamed `PrometheusBuilder::build_with_exporter` to `build`.
- `InstallError` is now `BuildError`, and contains many more variants with hopefully) better error
  messages for understanding when something went wrong.
- Most builder methods are now fallible to help avoid runtime panics for invalid data given when
  building, and to better surface this upfront to users.
- Rendered output for histograms is now stable, based on the order in which a given key
  (name/labels) was recorded.

### Fixed

- Label keys and values, as well as metric descriptions, are now correctly sanitized according to
  the Prometheus [data model](https://prometheus.io/docs/concepts/data_model/) and [exposition
  format](https://github.com/prometheus/docs/blob/main/content/docs/instrumenting/exposition_formats.md).
  ([#248](https://github.com/metrics-rs/metrics/issues/248))
- Metric descriptions are correctly mapped to metrics whose names have been modified during
  sanitization.
- Histograms are now correctly removed when they exceed the idle timeout.

## [0.7.0] - 2021-12-16

### Changed

- Calling `PrometheusBuilder::install` inside a Tokio runtime will spawn the exporter on that
  runtime rather than spawning a new runtime. ([#251](https://github.com/metrics-rs/metrics/pull/251))

## [0.6.1] - 2021-09-16

### Changed

- Simple release to bump dependencies.

## [0.6.0] - 2021-07-15

### Added

- Support for pushing to a Push Gateway. ([#217](https://github.com/metrics-rs/metrics/pull/217))

## [0.5.0] - 2021-05-18

### Added

- `PrometheusBuilder::add_allowed`, which enables the exporter to be configured with a
  list of IP addresses or subnets that are allowed to connect. By default, no restrictions
  are enforced.

## [0.4.0] - 2021-05-03

### Changed

- Bumped `metrics` dependency to `0.15` and updated the necessary APIs.

## [0.3.0] - 2021-02-02

### Changed

- Bumped `metrics` dependency to `0.14`.

## [0.2.0] - 2021-01-23

### Changed

- Switched from `MetricKind` for `MetricKindMask` for `PrometheusBuilder::idle_timeout`.

## [0.1.0] - 2021-01-22

### Added

- Genesis of the crate.
