# metrics

[![conduct-badge][]][conduct] [![license-badge][]](#license) [![discord-badge][]][discord] ![last-commit-badge][] ![contributors-badge][]

[conduct-badge]: https://img.shields.io/badge/%E2%9D%A4-code%20of%20conduct-blue.svg
[license-badge]: https://img.shields.io/badge/license-MIT-blue
[conduct]: https://github.com/metrics-rs/metrics/blob/master/CODE_OF_CONDUCT.md
[discord-badge]: https://img.shields.io/discord/500028886025895936
[discord]: https://discord.gg/XmDVes
[last-commit-badge]: https://img.shields.io/github/last-commit/metrics-rs/metrics
[contributors-badge]: https://img.shields.io/github/contributors/metrics-rs/metrics


The Metrics project: a metrics ecosystem for Rust.

## code of conduct

**NOTE**: All conversations and contributions to this project shall adhere to the [Code of Conduct][conduct].

# what's it all about?

Running applications in production can be hard when you don't have insight into what the application is doing.  We're lucky to have so many good system monitoring programs and services to show us how our servers are performing, but we still have to do the work of instrumenting our applications to gain deep insight into their behavior and performance.

_Metrics_ makes it easy to instrument your application to provide real-time insight into what's happening.  It provides a number of practical features that make it easy for library and application authors to start collecting and exporting metrics from their codebase.

# why would I collect metrics?

Some of the most common scenarios for collecting metrics from an application:
- see how many times a codepath was hit
- track the time it takes for a piece of code to execute
- expose internal counters and values in a standardized way

Importantly, this works for both library authors and application authors.  If the libraries you use are instrumented, you unlock the power of being able to collect those metrics in your application for free, without any extra configuration.  Everyone wins, and learns more about their application performance at the end of the day.

# project goals

Firstly, we want to establish standardized interfaces by which everyone can interoperate: this is the goal of the `metrics` and `metrics-core` crates.

`metrics` provides macros similar to `log`, which are essentially zero cost and invisible when not in use, but automatically funnel their data when a user opts in and installs a metrics recorder.  This allows library authors to instrument their libraries without needing to care which metrics system end users will be utilizing.

`metrics-core` provides foundational traits for core components of the metrics ecosystem, primarily the output side.  There are a large number of output formats and transports that application authors may consider or want to use.  By focusing on the API boundary between the systems that collect metrics and the systems they're exported to, these pieces can be easily swapped around depending on the needs of the end user.

Secondly, we want to provide a best-in-class reference runtime: this is the goal of the `metrics-runtime` crate.

Unfortunately, a great interface is no good without a suitable implementation, and we want to make sure that for users looking to instrument their applications for the first time, that they have a batteries-included option that gets them off to the races quickly.  The `metrics-runtime` crate provides a best-in-class implementation of a metrics collection system, including support for the core metric types -- counters, gauges, and histograms -- as well as support for important features such as scoping, labels, flexible approaches to recording, and more.

On top of that, collecting metrics isn't terribly useful unless you can export those values, and so `metrics-runtime` pulls in a small set of default observers and exporters to allow users to quickly set up their application to be observable by their existing downstream metrics aggregation/storage.

# project layout

The Metrics project provides a number of crates for both library and application authors.

If you're a library author, you'll only care about using [`metrics`] to instrument your library.  If you're an application author, you'll primarily care about [`metrics-runtime`], but you may also want to use [`metrics`] to make instrumenting your own code even easier.

Overall, this repository is home to the following crates:

* [`metrics`][metrics]: A lightweight metrics facade, similar to [`log`](https://docs.rs/log).
* [`metrics-core`][metrics-core]: Foundational traits for interoperable metrics libraries.
* [`metrics-runtime`][metrics-runtime]: A batteries-included metrics library.
* [`metrics-exporter-http`][metrics-exporter-http]: A metrics-core compatible exporter for serving metrics over HTTP.
* [`metrics-exporter-log`][metrics-exporter-log]: A metrics-core compatible exporter for forwarding metrics to logs.
* [`metrics-observer-json`][metrics-observer-json]: A metrics-core compatible observer that outputs JSON.
* [`metrics-observer-yaml`][metrics-observer-yaml]: A metrics-core compatible observer that outputs YAML.
* [`metrics-observer-prometheus`][metrics-observer-prometheus]: A metrics-core compatible observer that outputs the Prometheus exposition format.
* [`metrics-util`][metrics-util]: Helper types/functions used by the metrics ecosystem.

# contributing

We're always looking for users who have thoughts on how to make metrics better, or users with interesting use cases.  Of course, we're also happy to accept code contrbutions for outstanding feature requests! 😀

We'd love to chat about any of the above, or anything else, really!  You can find us over on [Gitter](https://gitter.im/metrics-rs/community).

[metrics]: https://github.com/metrics-rs/metrics/tree/master/metrics
[metrics-core]: https://github.com/metrics-rs/metrics/tree/master/metrics-core
[metrics-runtime]: https://github.com/metrics-rs/metrics/tree/master/metrics-runtime
[metrics-exporter-http]: https://github.com/metrics-rs/metrics/tree/master/metrics-exporter-http
[metrics-exporter-log]: https://github.com/metrics-rs/metrics/tree/master/metrics-exporter-log
[metrics-observer-json]: https://github.com/metrics-rs/metrics/tree/master/metrics-observer-json
[metrics-observer-yaml]: https://github.com/metrics-rs/metrics/tree/master/metrics-observer-yaml
[metrics-observer-prometheus]: https://github.com/metrics-rs/metrics/tree/master/metrics-observer-prometheus
[metrics-util]: https://github.com/metrics-rs/metrics/tree/master/metrics-util
