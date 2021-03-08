# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

## [0.6.2] - 2021-03-08
### Changed
- Fixed issue with ordering on `CompositeKey`. (#182)

## [0.6.1] - 2021-02-07
### Added
- Added `AbsoluteLayer` for supporting counters reported via absolute value.

## [0.6.0] - 2021-02-02
### Changed
- Bumped `metrics` dependency to `0.14`.

## [0.5.0] - 2021-01-23
### Changed
- `MetricKind` is now `MetricKindMask`, and `MetricKind` is used to define the source side i.e.
  `MetricKindMask` is used to match against `MetricKind`.

## [0.4.1] - 2021-01-22
- No changes.

## [0.4.0] - 2021-01-22
### Added
- Added multiple utility layers: prefix, filter, and fanout.
- Added `CompositeKey` for defining a metric kind/key combination.
- Added `Histogram` for defining a bucket-based histogram.
- Added `Registry` for storing metric handles mapped to a given key.
- Added `Summary`, based on DDSketch, as a more lightweight alternative to `hdrhistogram`.
- Added `DebuggingRecorder` to help tet various types that depend on a `Recorder`.
- Added `Recency` for tracking idle metrics.

### Removed
- Removed `StreamingIntegers` as we no longer use it, and `compressed_vec` is a better option.
- Removed `MetricsTree` as we no longer use it.

### Changed
- `AtomicBucket` now exposes `clear` and `clear_with` for both emptying the bucket and reading the
  values that were cleared, allowing behavior similiar to atomically swapping a fresh data structure
  in for another.

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
