# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

## [0.4.0] - 2021-01-22
### Removed
- Removed `StreamingIntegers` as we no longer use it, and `compressed_vec` is a better option.

## [0.3.1] - 2019-11-21
### Changed
- Updated to crossbeam-epoch 0.8 and switched directly to crossbeam-utils for tests.

## [0.3.0] - 2019-04-30
### Added
- `MetricTree` allows storing hierarchical metric values while being serializable by serde. (#38)

## [0.2.1] - 2019-06-12
Erroneously bumped/published.  No changes.

## [0.2.0] - 2019-06-11
### Added
- `StreamingIntegers` allows holding a set of integers in a compressed format in-memory. (#13)
- `AtomicBucket` provides an append-only list of items that can be snapshot without stopping writes. (#13)

## [0.1.0] - 2019-04-23
### Added
- Effective birth of the crate.
