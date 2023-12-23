# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

## [0.7.1] - 2023-12-23

## [0.7.0] - 2023-04-16

### Changed

- Bump MSRV to 1.61.0.

### Fixed

- Type paths are now fully qualified in all macros to avoid issues with local import scopes having a
  pre-existing `metrics` module.

## [0.6.0] - 2022-07-22

### Changed

- Updated the describe macros to support the recent change to taking `metrics::SharedString` instead
  of `&'static str` for description strings.

## [0.5.1] - 2022-02-06

Maintenance release.

## [0.5.0] - 2022-01-14

### Added
- When describing a metric, a constant can now be used for the description itself.
- Label keys can now be general expressions i.e. constants or variables.  Due to limitations in
  how procedural macros work, and the facilities available in stable Rust for const traits, even
  `&'static str` constants will cause allocations when used for emitting a metric.

### Changed
- Correctly scoped the required features of various dependencies to reduce build times/transitive dependencies.
- Updated macros to coincide with the update to `metrics` for metric handles.  This includes
  renaming `register_*` macros to `describe_*`, which are purely for providing data that describes a
  metric but does not initialize it in any way, and providing new `register_*` macros which do
  initialize a metric.
- Updated the `describe_*` macros -- née `register_*` -- to require a description, and an optional
  unit.  As describing a metric does not register it in the sense of ensuring that it is present on
  the output of an exporter, having the description be optional no longer makes sense.
- Additionally, the describe macros no longer take labels.  In practice, varying the description of
  a specific metric based on label values would be counter-intuitive, and to support this corner
  case requires adds significant complexity to the macro parsing logic.

### Removed
- Two unecessary dependencies, `lazy_static` and `regex`.

## [0.4.1] - 2021-12-16

### Changed
- Removed unnecessary `proc-macro-hack` dependency.

## [0.4.0] - 2021-05-18

### Changed
- Updates to macros to support the removal of `NameParts` and related machinery.

## [0.3.0] - 2021-05-03

### Changed
- Updates to macros to support changes in `Recorder` around how keys are taken.

## [0.2.0] - 2021-02-02
### Changed
- Added support for owned strings as metric names. [#170](https://github.com/metrics-rs/metrics/pull/170)

## [0.1.0] - 2021-01-22
### Added
- Effective birth of the crate.
