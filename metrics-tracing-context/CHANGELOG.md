# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

## [0.11.0] - 2022-05-30

### Added

- A new label filter, `Allowlist`, to only collect labels which are present in the list. ([#288](https://github.com/metrics-rs/metrics/pull/288))

### Changed

- Bumped the dependency on `metrics` to deal with a public API change.

## [0.10.0] - 2022-01-14

### Changed
- Updated various dependencies in order to properly scope dependencies to only the necessary feature
  flags, and thus optimize build times and reduce transitive dependencies.
- Updated to the new handle-based design of `metrics`.

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
