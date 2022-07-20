![Metrics - High-performance, protocol-agnostic instrumentation][splash]

[splash]: https://raw.githubusercontent.com/metrics-rs/metrics/main/assets/splash.png

[![Code of Conduct][conduct-badge]][conduct]
[![MIT licensed][license-badge]](#license)
[![Documentation][docs-badge]][docs]
[![Discord chat][discord-badge]][discord]
![last-commit-badge][]
![contributors-badge][]

[conduct-badge]: https://img.shields.io/badge/%E2%9D%A4-code%20of%20conduct-blue.svg
[conduct]: https://github.com/metrics-rs/metrics/blob/master/CODE_OF_CONDUCT.md
[license-badge]: https://img.shields.io/badge/license-MIT-blue
[docs-badge]: https://docs.rs/metrics/badge.svg
[docs]: https://docs.rs/metrics
[discord-badge]: https://img.shields.io/discord/500028886025895936
[discord]: https://discord.gg/eTwKyY9
[last-commit-badge]: https://img.shields.io/github/last-commit/metrics-rs/metrics
[contributors-badge]: https://img.shields.io/github/contributors/metrics-rs/metrics


## code of conduct

**NOTE**: All conversations and contributions to this project shall adhere to the [Code of Conduct][conduct].

# what's it all about?

Running applications in production can be hard when you don't have insight into what the application is doing.  We're lucky to have so many good system monitoring programs and services to show us how our servers are performing, but we still have to do the work of instrumenting our applications to gain deep insight into their behavior and performance.

`metrics` makes it easy to instrument your application to provide real-time insight into what's happening.  It provides a number of practical features that make it easy for library and application authors to start collecting and exporting metrics from their codebase.

# why would I collect metrics?

Some of the most common scenarios for collecting metrics from an application:
- see how many times a codepath was hit
- track the time it takes for a piece of code to execute
- expose internal counters and values in a standardized way

Importantly, this works for both library authors and application authors.  If the libraries you use are instrumented, you unlock the power of being able to collect those metrics in your application for free, without any extra configuration.  Everyone wins, and learns more about their application performance at the end of the day.

# project layout

The Metrics project provides a number of crates for both library and application authors.

If you're a library author, you'll only care about using [`metrics`][metrics] to instrument your library.  If you're an application author, you'll likely also want to instrument your application, but you'll care about "exporters" as a means to take those metrics and ship them somewhere for analysis.

Overall, this repository is home to the following crates:

* [`metrics`][metrics]: A lightweight metrics facade, similar to [`log`][log].
* [`metrics-macros`][metrics-macros]: Procedural macros that power `metrics`.
* [`metrics-tracing-context`][metrics-tracing-context]: Allow capturing [`tracing`][tracing] span
  fields as metric labels.
* [`metrics-exporter-tcp`][metrics-exporter-tcp]: A `metrics`-compatible exporter for serving metrics over TCP.
* [`metrics-exporter-prometheus`][metrics-exporter-prometheus]: A `metrics`-compatible exporter for
  serving a Prometheus scrape endpoint.
* [`metrics-util`][metrics-util]: Helper types/functions used by the `metrics` ecosystem.

# community integrations

As well, there are also some community-maintained exporters and other integrations:

* [`metrics-exporter-statsd`][metrics-exporter-statsd]: A `metrics`-compatible exporter for sending metrics via StatsD.
* [`metrics-exporter-newrelic`][metrics-exporter-newrelic]: A `metrics`-compatible exporter for sending metrics to New Relic.
* [`opinionated_metrics`][opinionated-metrics]: Opinionated interface to emitting metrics for CLi/server applications, based on `metrics`.

## MSRV

Minimum Supported Rust version is **1.56.1**.
It is enforced in CI.

### policy for bumping MSRV

* The last 4 stable releases must always be supported
* Goal is to try and support older versions where possible (not opting in to newer versions just to use a new helper method on standard library types, etc)
* Do not bump the MSRV for newer versions of dependencies in core crates (metrics and metrics-util)

# contributing

To those of you who have already contributed to `metrics` in some way, shape, or form: **a big, and continued, "thank you!"** ❤️

To everyone else that we haven't had the pleasure of interacting with: we're always looking for thoughts on how to make `metrics` better, or users with interesting use cases.  Of course, we're also happy to accept code contributions for outstanding feature requests directly. 😀

We'd love to chat about any of the above, or anything else related to metrics. Don't hesitate to file an issue on the repository, or come and chat with us over on [Discord](https://discord.gg/eTwKyY9).

[metrics]: https://github.com/metrics-rs/metrics/tree/main/metrics
[metrics-macros]: https://github.com/metrics-rs/metrics/tree/main/metrics-macros
[metrics-tracing-context]: https://github.com/metrics-rs/metrics/tree/main/metrics-tracing-context
[metrics-exporter-tcp]: https://github.com/metrics-rs/metrics/tree/main/metrics-exporter-tcp
[metrics-exporter-prometheus]: https://github.com/metrics-rs/metrics/tree/main/metrics-exporter-prometheus
[metrics-util]: https://github.com/metrics-rs/metrics/tree/main/metrics-util
[log]: https://docs.rs/log
[tracing]: https://tracing.rs
[metrics-exporter-statsd]: https://docs.rs/metrics-exporter-statsd
[metrics-exporter-newrelic]: https://docs.rs/metrics-exporter-newrelic
[opinionated-metrics]: https://docs.rs/opinionated_metrics
