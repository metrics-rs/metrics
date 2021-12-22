# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

### Changed
- Updated various dependencies in order to properly scope dependencies to only the necessary feature
  flags, and thus optimize build times and reduce transitive dependencies.
- Updated to the new handle-based design of `metrics`.

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
