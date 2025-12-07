# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

## [0.18.1] - 2025-12-07

### Fixed

- Fixed a bug where native histograms were incorrectly skipped when serialized the Protocol Buffers
  payload. ([#649](https://github.com/metrics-rs/metrics/pull/649))

## [0.18.0] - 2025-11-28

### Added

- Added `with_recommended_naming` to `PrometheusBuilder`, which when set to `true` will use the recommended naming
  convention for Prometheus metrics: suffixing counters with `_total` and ensuring unit suffixes are present. Users
  should prefer this going forward, and `set_enable_unit_suffix` is deprecated and will be removed in a future release
  in favor of `with_recommended_naming`. ([#596](https://github.com/metrics-rs/metrics/pull/596))
- Added support for emitting metrics in the Protocol Buffers-based scrape format. The exporter automatically detects
  which format to render responses in based on the presence/value of the `Accept` header in requests. Additionally, a
  new method, `PrometheusHandle::render_protobuf`, has been added to render metrics in this format, similar to
  `PrometheusHandle::render`. ([#602](https://github.com/metrics-rs/metrics/pull/602))
- Added `LabelSet`, which provides a wrapper around label key/value pairs to ensure consistent sanitization.
  ([#605](https://github.com/metrics-rs/metrics/pull/605))
- Added anew feature flag, `push-gateway-no-tls-provider`, to allow enabling push gateway support without including a
  default TLS provider via `rustls`. Users enabling this flag must install a default TLS provider for `rustls` via
  [`rustls::crypto::CryptoProvider::install_default`](https://docs.rs/rustls/latest/rustls/crypto/struct.CryptoProvider.html#method.install_default).
  ([#607](https://github.com/metrics-rs/metrics/pull/607))
- Added experimental support for native histograms. Users can enable native histograms using the existing matcher
  approach by calling `PrometheusBuilder::set_native_histogram_for_metric`. Native histograms are only rendered when the
  Protocol Buffers-based scrape format is in use. ([#610](https://github.com/metrics-rs/metrics/pull/610))
- Added a new method (`render_to_write`) to `PrometheusHandle` that allows writing directly to a `Write` implementation.
  ([#641](https://github.com/metrics-rs/metrics/pull/641))

### Changed

- Slightly updated the logic for rendering Prometheus outputs to be more incremental where possible to allow `render_to_write`
  to be roughly constant memory usage regardless of the overall number of metrics being rendered.
  ([#641](https://github.com/metrics-rs/metrics/pull/641))

### Fixed

- Fixed a bug where the `_total` suffix for counters was not being appended to the HELP or TYPE lines for counters.
  ([#597](https://github.com/metrics-rs/metrics/pull/597))

## [0.17.2] - 2025-06-20

### Changed

- Update `metrics-util` to `0.20`.

## [0.17.1] - 2025-06-18

### Fixed

- Apply unit suffixes in the correct portion of metric names when unit suffixing is enabled.
  ([#582](https://github.com/metrics-rs/metrics/pull/582))

## [0.17.0] - 2025-04-20

### Changed

- Updated `rand` to `0.9`. ([#556](https://github.com/metrics-rs/metrics/pull/556))
- Bumped `thiserror` to `2.0`. ([#572](https://github.com//metrics-rs/metrics/pull/572))
- Added new flag, `use_http_post_method`, to `PrometheusBuilder::with_push_gateway`, to allow changing the HTTP method
  used for pushing from PUT to POST. This enables usage with systems like Vector which don't natively support PUT.
  ([#574](https://github.com/metrics-rs/metrics/pull/574))
- Render scrape endpoint/push gateway payloads in a blocking thread to avoid blocking regular Tokio executor threads.
  ([#576](https://github.com/metrics-rs/metrics/pull/576))

## [0.16.2] - 2025-01-31

### Fixed

- Fixed a bug where the configurable unit suffix was not properly applied to the "sum" and "count" lines for histograms.
  The prior release will be yanked as this represented a backwards-incompatible change.

## [0.16.1] - 2025-01-06

### Changed

- Updated the crate-level documentation, and the documentation for `PrometheusBuilder::build_recorder` and
  `PrometheusBuilder::install_recorder`, to call out the requirements around running upkeep periodically.
  ([#537](https://github.com/metrics-rs/metrics/pull/537))
- Updated to new version of `metrics-util`.

### Added

- Added support for suffixing metric names based on their configured units. ([#535](https://github.com/metircs-rs/metrics/pull/535))

## [0.16.0] - 2024-10-12

### Added

- Added `Debug` derive to numerous types. ([#504](https://github.com/metrics-rs/metrics/pull/504))

### Changed

- Fixed a number of Clippy lints. ([#510](https://github.com/metrics-rs/metrics/pull/510))
- Bump MSRV to 1.71.1. ([#530](https://github.com/metrics-rs/metrics/pull/530))

## [0.15.3] - 2024-07-13

Republishing 0.15.2 as 0.15.3 to fix an incorrect publish.

## [0.15.2] - 2024-07-13

### Added

- Added support to use a UDS listener for the HTTP gateway mode.
  ([#498](https://github.com/metrics-rs/metrics/pull/498))

### Changed

- Update the `Content-Type` response header to `text/plain`, matching the Exposition format
  specification. ([#496](https://github.com/metrics-rs/metrics/pull/496))

## [0.15.1] - 2024-06-24

### Changed

- Switch to `rustls`. ([#489](https://github.com/metrics-rs/metrics/pull/489))

## [0.15.0] - 2024-05-27

### Changed

- Bump MSRV to 1.70.0.

## [0.14.0] - 2024-03-16

### Added

- Users can now configure the number of buckets, and bucket widths, for rolling summaries. ([#444](https://github.com/metrics-rs/metrics/pull/444))
- Added support to run exporter "upkeep" in the background to periodically drain histograms and help
  avoid unbounded memory growth over time. ([#460](https://github.com/metrics-rs/metrics/pull/460))

### Changed

- Upgrade to `hyper` 1.x. ([#450](https://github.com/metrics-rs/metrics/pull/450))
- Enabled upkeep by default, with a five second interval. ([#460](https://github.com/metrics-rs/metrics/pull/460))

## [0.13.1] - 2024-02-11

### Added

- A new scrape endpoint path, `/health`, which returns a a basic response to indicate endpoint health. ([#435](https://github.com/metrics-rs/metrics/pull/435))

### Changed

- Bumped the `indexmap` dependency to the latest version. ([#439](https://github.com/metrics-rs/metrics/pull/439))

## [0.13.0] - 2023-12-24

### Added

- Support for using HTTPS in Push Gateway mode.
  ([#392](https://github.com/metrics-rs/metrics/pull/392))

### Changed

- Bump MSRV to 1.65.0.

## [0.12.2] - 2023-12-13

### Fixed

- Fixed overflow/underflow panic with time moving backwards ([#423](https://github.com/metrics-rs/metrics/pull/423))

## [0.12.1] - 2023-05-09

### Added

- Support for specifying a username/password for HTTP Basic Authentication when pushing to a Push
  Gateway. ([#366](https://github.com/metrics-rs/metrics/pull/366))

## [0.12.0] - 2023-04-16

### Changed

- Bump MSRV to 1.61.0.
- Switch to `metrics`-exposed version of `AtomicU64`.

## [0.11.0] - 2022-07-20

### Changed

- Aggregated summaries are now rolling, allowing oldering data points to expire and quantile values
  to reflect the recent past rather than the lifetime of a histogram.
  ([#306](https://github.com/metrics-rs/metrics/pull/306))

  They have a default width of three buckets, with each bucket being 20 seconds wide. This means
  only the last 60 seconds of a histogram -- in 20 second granularity -- will contribute to the
  quantiles emitted.

  We'll expose the ability to tune these values in the future.
- Switched to using `portable_atomic` for 64-bit atomics on more architectures.
  ([#313](https://github.com/metrics-rs/metrics/pull/313))


## [0.10.0] - 2022-05-30

### Fixed

- In some cases, metric names were being "sanitized" when they were already valid.
  ([#290](https://github.com/metrics-rs/metrics/pull/290), [#296](https://github.com/metrics-rs/metrics/pull/296))

## [0.9.0] - 2022-03-10

### Added

- New top-level module, `formatting`, which exposes many of the helper methods used to sanitize and
  render the actual Prometheus exposition format. ([#285](https://github.com/metrics-rs/metrics/pull/285))

## [0.8.0] - 2022-01-14

### Added

- New builder method, `PrometheusBuilder::install_recorder`, which builds and installs the
  recorder and returns a `PrometheusHandle` that can be used to interact with the recorder.

### Changed

- Updated various dependencies in order to properly scope dependencies to only the necessary feature
  flags, and thus optimize build times and reduce transitive dependencies.
- Updated to the new handle-based design of `metrics`.
- Renamed `tokio-exporter` feature flag to `http-listener`.
- Renamed `PrometheusBuilder::build` to `build_recorder`.
- Renamed `PrometheusBuilder::build_with_exporter` to `build`.
- `InstallError` is now `BuildError`, and contains many more variants with hopefully) better error
  messages for understanding when something went wrong.
- Most builder methods are now fallible to help avoid runtime panics for invalid data given when
  building, and to better surface this upfront to users.
- Rendered output for histograms is now stable, based on the order in which a given key
  (name/labels) was recorded.

### Fixed

- Label keys and values, as well as metric descriptions, are now correctly sanitized according to
  the Prometheus [data model](https://prometheus.io/docs/concepts/data_model/) and [exposition
  format](https://github.com/prometheus/docs/blob/main/content/docs/instrumenting/exposition_formats.md).
  ([#248](https://github.com/metrics-rs/metrics/issues/248))
- Metric descriptions are correctly mapped to metrics whose names have been modified during
  sanitization.
- Histograms are now correctly removed when they exceed the idle timeout.

## [0.7.0] - 2021-12-16

### Changed

- Calling `PrometheusBuilder::install` inside a Tokio runtime will spawn the exporter on that
  runtime rather than spawning a new runtime. ([#251](https://github.com/metrics-rs/metrics/pull/251))

## [0.6.1] - 2021-09-16

### Changed

- Simple release to bump dependencies.

## [0.6.0] - 2021-07-15

### Added

- Support for pushing to a Push Gateway. ([#217](https://github.com/metrics-rs/metrics/pull/217))

## [0.5.0] - 2021-05-18

### Added

- `PrometheusBuilder::add_allowed`, which enables the exporter to be configured with a
  list of IP addresses or subnets that are allowed to connect. By default, no restrictions
  are enforced.

## [0.4.0] - 2021-05-03

### Changed

- Bumped `metrics` dependency to `0.15` and updated the necessary APIs.

## [0.3.0] - 2021-02-02

### Changed

- Bumped `metrics` dependency to `0.14`.

## [0.2.0] - 2021-01-23

### Changed

- Switched from `MetricKind` for `MetricKindMask` for `PrometheusBuilder::idle_timeout`.

## [0.1.0] - 2021-01-22

### Added

- Genesis of the crate.
