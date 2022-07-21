# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

## [0.14.0] - 2022-07-20

### Changed

- Updated `sketches-ddsketch` to `0.2.0`.
- Switched to using `portable_atomic` for 64-bit atomics on more architectures. (#313)

## [0.13.0] - 2022-05-30

### Fixed

- In `Summary`, some quantiles were previously mapped to an incorrect rank at low sample counts, leading to large swings
  in estimated values. ([#304](https://github.com/metrics-rs/metrics/pull/304))

### Changed

- Bumped the dependency on `metrics` to deal with a public API change.

## [0.12.1] - 2022-05-02

### Added

- A new per-thread mode for `DebuggingRecorder` that allows recording metrics on a per-thread basis to better supporting
  the testing of metrics in user applications where many tests are concurrently emitting metrics.

## [0.12.0] - 2022-03-10

### Added

- New storage type, `GenerationalStorage`, that provides the generational behavior of
  `GenerationalAtomicStorage`, nee `GenerationalPrimitives`, without being inherently coupled to
  using atomic storage.
### Changed

- Renamed `Primitives` to `Storage`, and publicly exposed it so that users can implement their own
  storage strategies.
- Renamed `StandardPrimitives` and `GenerationalPrimitives` to `AtomicStorage` and
  `GenerationalAtomicStorage`, respectively.
- Created a new top-level module, `registry`, that encompasses `Registry` and all `Registry`-related
  and dependent types.
- Exposed `DefaultHashable` publicly.
- Debugging utilities have been moved to a new `debugging` module, and `Snapshot` is now public.

## [0.11.1] - 2022-02-20

### Fixed

- `Recency` incorrectly failed to keep track of the latest generation when a previously-observed
  metric had changed generations since the first time it was observed.

## [0.11.0] - 2022-01-14

### Changed

- Updated various dependencies in order to properly scope dependencies to only the necessary feature
  flags, and thus optimize build times and reduce transitive dependencies.
- Many types are now behind their own feature flags to allow for optimizing build times and
  dependency tree.
- Updated to the new handle-based design of `metrics`.
- Updated `AtomicBucket` to use `MaybeUninit` and better documented the safety invariants in the
  various areas that use `unsafe`.
- `AtomicBucket` now correctly drops only initialized slots in a `Block`.

## [0.10.2] - 2021-12-12

### Changed

- Bump `atomic-shim` from 0.1 to 0.2.

## [0.10.1] - 2021-09-16

### Changed

- Simple release to bump dependencies.

## [0.10.0] - 2021-07-13

### Changed

- Bumped `metrics` dependency to correspond to the change in key hasher.

## [0.9.1] - 2021-05-24

### Changed

- Pin `crossbeam-epoch` to the correct version where `Atomic::compare_exchange` support was added.

## [0.9.0] - 2021-05-19

### Changed

- Reworked `Registry` to make generation tracking a configurable property at the type level.

## [0.8.0] - 2021-05-18

### Added

- New layer -- `Router` -- for routing specific metrics to target downstream recorders.
- `Registry::clear` allows clearing all metrics from the registry.

### Changed

- Updated all deprecated usages of `crossbeam_epoch::Atomic<T>::compare_and_set` to `compare_exchange`.

## [0.7.0] - 2021-05-03

### Changed

- Switched `Registry` over to supporting `&Key`.
- Switched from `dashmap` to `hashbrown`/`parking_lot` for `Registry`.
- Updated all layers to support the change from `Key` to `&Key` in `metrics::Recorder`.`

### Added

- Support for using the pre-hashed value of `Key` to speed up `Registry` operations.

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
