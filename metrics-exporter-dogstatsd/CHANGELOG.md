# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

## [0.9.1] - 2025-03-25

### Fixed

- The internal forwarder state was left in an inconsistent state after a connection failure was encountered.
  ([#563](https://githib.com/metrics-rs/metrics/pull/563))

### Added

- Added support for configuring global labels, as well as prefix.
  ([#555](https://githib.com/metrics-rs/metrics/pull/555))

## [0.9.0] - 2025-01-19

### Added

- Genesis of the new version of this crate. This version of the exporter is a successor to the original
  `metrics-exporter-dogstatsd` exporter written by [Valentino Volonghi](https://github.com/dialtone), and starts after
  the last version of that crate (`0.8.0`) to indicate the SemVer-incompatible changes that have been made. This new
  crate is MIT licenses just as the original one was. The old code can be found
  [here](https://github.com/dialtone/metrics-exporter-dogstatsd).
