# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

## [0.9.0] - 2021-12-16

### Changed
- Bumped dependency on `tracing-subscriber` to `0.3`. ([#249](https://github.com/metrics-rs/metrics/pull/249))

## [0.8.1] - 2021-11-02

### Changed
- Updated all dependencies to remove default features and only use necessary features for the crate itself.

## [0.8.0] - 2021-07-19

### Changed
- Improved performance by memoizing/denormalizing fields, pooling label storage, and improving how
  the current span is accessed. ([#224](https://github.com/metrics-rs/metrics/pull/224))

## [0.7.0] - 2021-07-14

## [0.6.0] - 2021-05-19

## [0.5.0] - 2021-05-18

## [0.4.0] - 2021-05-03

### Changed
- Bumped `metrics` dependency to `0.15` and updated the necessary APIs.
## [0.3.0] - 2021-02-02
### Changed
- Bumped `metrics` dependency to `0.14`.

## [0.2.0] - 2021-01-23
### Changed
- Switched from `MetricKind` to `MetricKindMask`.

## [0.1.0] - 2021-01-22
### Added
- Genesis of the crate.
