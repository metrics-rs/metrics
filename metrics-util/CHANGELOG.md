# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

## [0.17.0] - 2024-05-27

### Changed

- Bump MSRV to 1.70.0.
- `Snapshotter` now implements `Clone`.
  ([#472](https://github.com/metrics-rs/metrics/pull/472))
- Relaxed some bounds for different categories of methods on `Registry<K, S>`.
  ([#484](https://github.com/metrics-rs/metrics/pull/484))

## [0.16.3] - 2024-03-16

### Added

- New set of methods on `Registry` for getting a metric handle if it exists. ([#457](https://github.com/metrics-rs/metrics/pull/457))
- New set of methods on `Registry` for retaining metric handles that match a given predicate.
  ([#461](https://github.com/metrics-rs/metrics/pull/461))

### Fixed

- Bump `ahash` back to `0.8.8` to remove range constraint after an upstream fix was provided to
  remove the unnecessary MSRV bump.

## [0.16.2] - 2024-02-11

### Fixed

- Lock down the version of `ahash` to avoid unnecessary MSRV bumping.

## [0.16.1] - 2024-02-11

### Changed

- Bumped the `indexmap` and `hashbrown` dependencies to their latest versions. ([#438](https://github.com/metrics-rs/metrics/pull/438), [#439](https://github.com/metrics-rs/metrics/pull/439))

## [0.16.0] - 2023-12-24

### Fixed

- Fixed the `Debug` implementation for `bucket::Block<T>` which represented both an unsafe and
  logically incorrect usage of `crossbeam-epoch.`

### Changed

- Bump MSRV to 1.65.0.
- `RecoverableRecorder` no longer functions as a drop guard itself, and instead returns a new
  type, `RecoveryHandle<R>`, which provides that functionality. ([#414](https://github.com/metrics-rs/metrics/pull/414))

### Removed

- Support for per-thread mode in `DebuggingRecorder`. Users should now use
  `metrics::with_local_recorder` instead, which is inherently per-thread.

## [0.15.1] - 2023-07-02

### Added

- Added a new helper type, `RecoverableRecorder`, that allows installing a recorder and then
  recovering it later.

### Changed

- Update `aho-corasick` to `1.0`.
- Pinned `hashbrown` to `0.13.1` to avoid MSRV bump.

## [0.15.0] - 2023-04-16

### Changed

- Bump MSRV to 1.61.0.
- Switch to `metrics`-exposed version of `AtomicU64`.

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
