# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

## [0.22.3] - 2024-03-16

### Added

- Additional implementations of `IntoF64` for standard numerical types (`i8`, `u8`, `i16`, `u16`,
  `i32`, `u32`, and `f32`).

## [0.22.2] - 2024-03-16

### Fixed

- Bump `ahash` back to `0.8.8` to remove range constraint after an upstream fix was provided to
  remove the unnecessary MSRV bump.

## [0.22.1] - 2024-02-11

### Fixed

- Lock down the version of `ahash` to avoid unnecessary MSRV bumping.

## [0.22.0] - 2023-12-24

### Added

- Support for using `Arc<T>` with `Cow<'a, T>`.
  ([#402](https://github.com/metrics-rs/metrics/pull/402))

  This will primarily allow using `Arc<str>` for metric names and labels, where previously only
  `&'static str` or `String` were allowed. There's still work to be done to also support labels in
  this regard.

- Support for local recorders. ([#414](https://github.com/metrics-rs/metrics/pull/414))

  This is a large feature, and is documented in [RELEASES.md](RELEASES.md).

### Changed

- Make `Unit` methods return `&'static str` (instead of `&str`) where possible. ([#392](https://github.com/metrics-rs/metrics/pull/393))
- Bump MSRV to 1.65.0.
- `SetRecorderError` now returns the recorder given to `set_global_recorder` if another global
  recorder was already installed instead of leaking it. ([#414](https://github.com/metrics-rs/metrics/pull/414))

## [0.21.1] - 2023-07-02

### Added

- Added a `From` implementation for `KeyName` from `Cow<'static, str>`.
  ([#378](https://github.com/metrics-rs/metrics/pull/378))

### Removed

- Removed `metrics::set_recorder_racy` as it was intended to be used in `no_std` use cases, but
  `metrics` is not currently compatible in `no_std` environments, so keeping `set_recorder_racy`
  around was just API baggage.

## [0.21.0] - 2023-04-16

### Added

- A new module, `atomics`, exposes the atomic integer type that `CounterFn` and `GaugeFn` are
  implemented for. This will publicly re-export the type for usage by downstream crates. (Credit to
  [@repi](https://github.com/repi) for the original PR (#347) that did this.)

### Changed

- Bump MSRV to 1.61.0.
- `portable-atomic` is only used on 32-bit architectures.

### Removed

- Removed the `std-atomics` feature flag.

## [0.20.1] - 2022-07-22

### Changed

- Bumped the dependency on `metrics-macros` to correctly use the updated versions that are necessary
  for handling the recent `&'static str` -> `SharedString` change to `Recorder::describe_*`.

  We'll also yank 0.20.0 once this is released to avoid the patch version triggering a breaking
  change jump in transitive dependencies, and so people can't pick up a version of `metrics` that
  doesn't actually work as it should.

## [0.20.0] - 2022-07-20

### Changed

- Changed `Recorder::describe_*` to take `SharedString` instead of `&'static str` for descriptions. (#312)
- Implemented `CounterFn` and `GaugeFn` for `portable_atomic::AtomicU64` (#313)
- Moved implementations of `CounterFn` and `GaugeFn` for `std::sync::atomic::AtomicU64` behind a
  default feature flag.

## [0.19.0] - 2022-05-30

### Fixed

- Small typo in the documentation. ([#286](https://github.com/metrics-rs/metrics/pull/286))

### Changed

- Refactored the global recorder instance, namely around how it gets set and documenting the safety guarantees of
  methods related to setting and unsetting it. ([#302](https://github.com/metrics-rs/metrics/pull/302))
- Fixed an issue with pointer provenance in `metrics::Cow`. ([#303](https://github.com/metrics-rs/metrics/pull/303))

## [0.18.1] - 2022-03-10

### Added
- Slices of string key/value tuples can now be passed as the labels expression in macros. ([#277](https://github.com/metrics-rs/metrics/pull/277))

## [0.18.0] - 2022-01-14

### Added
- A new macro, `absolute_counter!`, for setting the value of a counter to an absolute value.
- A new wrapper type, `KeyName`, which encapsulates creating the name portion of a `Key`.  Existing
  methods for building a `Key`, as well as implicit conversion trait implementations, allow this to
  be a no-op.
- Label keys in macros can now be general expressions i.e. constants or variables.  Due to
  limitations in how procedural macros work, and the facilities available in stable Rust for const
  traits, even `&'static str` constants will cause allocations when used for emitting a metric.

### Changed
- Switched to metric handles through the `Recorder` API.
  ([#240](https://github.com/metrics-rs/metrics/pull/240)).  Due to the size of this change, the
  details are further documented and discussed in [RELEASES.md](RELEASES.md).
- `Unit` is now `Copy`.
- When describing a metric via the `describe_*` macros, the description is no longer optional.

### Removed
- Removed the `std` feature flag, as `metrics` depends too heavily on `std`-based types and as such
  was not meaningfully usaable when the `std` feature flag was disabled.  This will be revisited in
  the future.

## [0.17.1] - 2021-12-16

### Changed
- Removed unnecessary `proc-macro-hack` dependency.

## [0.17.0] - 2021-07-13

### Changed
- Switched from `t1ha` to `ahash` for the key hasher.

## [0.16.0] - 2021-05-18

### Removed
- `NameParts` has been removed to simplify metric names, again relying on a single string which is
  still backed by copy-on-write storage.

## [0.15.1] - 2021-05-03
### Changed
- Nothing.  Fixed an issue with using the wrong dependency version during a mass release of
  workspace crates.

## [0.15.0] - 2021-05-03
### Changed
- Switched from `Key` to `&Key` in `Recorder`.
- Refactored `KeyData` into `Key`.

### Added
- Metric keys are now pre-hashed/memoized where possible, which provides a massive speedup to
  hashing operations over time.
## [0.14.2] - 2021-02-13
### Added
- Implemented `Ord`/`PartialOrd` for various key-related types.

## [0.14.1] - 2021-02-02
### Added
- Minor documentation test updates for better coverage of owned strings used as metric names.

## [0.14.0] - 2021-02-02
### Changed
- Added support for owned strings as metric names. [#170](https://github.com/metrics-rs/metrics/pull/170)

## [0.13.1] - 2021-01-23
### Added
- Added conversion from `std::borrow::Cow<'static, str>` for `SharedString`.

## [0.13.0] - 2021-01-22
### Added
- New macros for registration: `register_counter!`, `register_gauge!`, `register_histogram!`.
- New macros for emission: `histogram!`, `increment_counter!`, `increment_gauge!`,
  `decrement_gauge!`.
- Added unit support to describe the unit of a given metric.

### Removed
- Dropped the `timing!` and `value!` macros in favor of `histogram!`.

### Changed
- All macros are now procedural macros instead of declarative macros.
- Gauges are now `f64` instead of `i64`.
- Histograms are now `f64` instead of `u64`.
- `Key` now split into `Key` and `KeyData` to better support constant expression metric keys via
  macro callsites.
- Significant overhaul of generated callsites via macros.

## [0.12.1] - 2019-11-21
### Changed
- Cost for macros dropped to almost zero when no recorder is installed. ([#55](https://github.com/metrics-rs/metrics/pull/55))

## [0.12.0] - 2019-10-18
### Changed
- Improved documentation. (#44, #45, #46)
- Renamed `Recorder::record_counter` to `increment_counter` and `Recorder::record_gauge` to `update_gauge`. ([#47](https://github.com/metrics-rs/metrics/pull/47))

## [0.11.1] - 2019-08-09
### Changed
- Fixed a bug with macros calling inner macros without a fully qualified name.

## [0.11.0] - 2019-07-29
### Added
- Life begins at 0.11.0 for this crate, after being renamed from `metrics-facade` to `metrics` to
  reflect the duality of `metrics` to the `log` crate. (#27)
