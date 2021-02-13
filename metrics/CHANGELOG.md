# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

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
