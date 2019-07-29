# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0] - 2019-07-29
### Added
- `Key` now supports labels. (#27)
- `Builder` for building observers in a more standardized way. (#30)

### Changed
- `Recorder` is now `Observer`. (#35)

## [0.4.0] - 2019-06-11
### Added
- Add `Key` as the basis for metric names. (#20)
- Add `AsNanoseconds` for defining types that can be used for start/end times. (#20)

## [0.3.1] - 2019-04-30
### Removed
- Removed extraneous import.

## [0.3.0] - 2019-04-30
### Added
- Added snapshot traits for composable snapshotting. (#8)

### Changed
- Reduced stuttering in type names. (#8)

## [0.2.0] - 2019-04-23
### Changed
- Changed from "exporter" to "recorder" in type names, documentation, etc.

## [0.1.2] - 2019-03-26
### Added
- Effective birth of the crate -- earlier versions were purely for others to experiment with. (#1)
