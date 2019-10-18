# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.12.0] - 2019-10-18
### Changed
- Rename `Sink::record_counter` to `increment_counter`, `Sink::record_gauge` to `update_gauge`. (#47)
- Rename `Receiver::get_controller` and `Receiver::get_sink` to `controller` and `sink`, respectively. (#48)
- Switch to the new updated `Recorder` trait from `metrics`. (#47)

### Fixed
- Fixed some broken tests and incorrect documentation. (#53)

## [0.11.0] - 2019-07-29
### Added
- Metrics now support labels. (#27)
- Add support for proxy metrics. (#39)

### Changed
- `metrics` becomes `metrics-runtime` after switching the runtime and the facade crate. (#27)
- Switch from "recorders" to "observers." (#35)

## [0.10.0] - 2019-06-11
### Changed
- Entirely remove the event loop and switch to pure atomics. (#13)

## [0.9.1] - 2019-05-01
### Added
- Expose exporters/recorders via a facade module in `metrics`. (#8)

## [0.9.0] - 2019-04-03
### Changed
- `hotmic` is renamed to `metrics. (#2)

## [0.8.2] - 2019-03-19
### Added
- Histograms now track the sum of all values they record, to support target systems like Prometheus.
- Added the ability to get percentiles as quantiles.  This is also to support target systems like Prometheus.  These are derived from the existing percentile values and so can have extra decimal precision.  This will be unified in a future breaking update.

## [0.8.1] - 2019-03-15
### Changed
- Fixed some issues with type visibility and documentation.

## [0.8.0] - 2019-03-15
### Changed
- Removed accessors from `Snapshot`.  It is not an opaque type that can be turned into an iterator which will provide access to typed metric values so that an external consumer can get all of the values in the snapshot, including their type, for proper exporting.
### Added
- A new "simple" snapshot type -- `SimpleSnapshot` -- which has easy-to-use accessors for metrics, identical to what `Snapshot` used to have.
- Allow retrieving snapshots asynchronously via `Controller::get_snapshot_async`.  Utilizes a oneshot channel so the caller can poll asynchronously.

## [0.7.1] - 2019-01-28
### Changed
- Fixed a bug where new sinks with the same scope would overwrite each others metrics. [#20](https://github.com/nuclearfurnace/hotmic/pull/20)

## [0.7.0] - 2019-01-27
### Changed
- Sink scopes can now be either a `&str` or `&[&str]`.
- Fixed a bug where the receiver loop ran its thread at 100%.

## [0.6.0] - 2019-01-24
### Changed
- Metrics auto-register themselves now. [#16](https://github.com/nuclearfurnace/hotmic/pull/16)

## [0.5.2] - 2019-01-19
### Changed
- Snapshot now implements [`Serialize`](https://docs.rs/serde/1.0.85/serde/trait.Serialize.html).

## [0.5.1] - 2019-01-19
### Changed
- Controller is now `Clone`.

## [0.5.0] - 2019-01-19
### Added
- Revamp API to provide easier usage. [#14](https://github.com/nuclearfurnace/hotmic/pull/14)

## [0.4.0] - 2019-01-14
Minimum supported Rust version is now 1.31.0, courtesy of switching to the 2018 edition.

### Changed
- Switch to integer-backed metric scopes. [#10](https://github.com/nuclearfurnace/hotmic/pull/10)
### Added
- Add clock support via `quanta`. [#12](https://github.com/nuclearfurnace/hotmic/pull/12)

## [0.3.0] - 2018-12-22
### Added
- Switch to crossbeam-channel and add scopes. [#4](https://github.com/nuclearfurnace/hotmic/pull/4)
