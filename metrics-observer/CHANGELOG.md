# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

### Fixed

- All addresses returned when trying to connect to the specified exporter endpoint will be tried, in
  order, instead of only trying the first and then giving up.
  ([#429](https://github.com/metrics-rs/metrics/pull/429))

## [0.2.0] - 2023-04-16

### Added

- Update `hdrhistogram`` dependency to 7.2

### Changed

- Bump MSRV to 1.61.0.
- Updated various dependencies in order to properly scope dependencies to only the necessary feature
  flags, and thus optimize build times and reduce transitive dependencies.
