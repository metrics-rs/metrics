# Releases
Unlike the [CHANGELOG](CHANGELOG.md), this file tracks more complicated changes that required
long-form description and would be too verbose for the changelog alone.

<!-- next-header -->

## [Unreleased] - ReleaseDate

- No notable changes.

## [0.21.1] - 2023-07-02

- No notable changes.

## [0.21.0] - 2023-04-16

- No notable changes.

## [0.20.1] - 2022-07-22

- No notable changes.

## [0.20.0] - 2022-07-20

- No notable changes.

## [0.19.0] - 2022-05-30

- No notable changes.

## [0.18.1] - 2022-03-10

- No notable changes.

## [0.18.0] - 2022-01-14

### Switch to metric handles
`metrics` has now switched to "metric handles."  This requires a small amount of backstory.

#### Evolution of data storage in `metrics`

Originally, [`metrics`][metrics] started its life as [`metrics-runtime`][metrics-runtime], which represented a batteries-included
implementation of metrics storage, with both a user-accessible API that allowed directly interacting
with the metrics as well as a mode for being installed as the global recorder for [`metrics`].

The user-accessible API was most commonly accessed via a helper type called `Sink`, which could both
create metric type-specific handles -- such as `Counter` or `Gauge` -- and could also perform
operations directly, such as `increment_counter`.

However, users still needed to both create the machinery supplied by `metrics-runtime` to handle
metrics storage, as well as the actual exporters to access those metrics, or send them to the
downstream system of choice.  We ultimately made the decision to bundle the metrics storage
capabilities into exporters directly by moving much of [`metrics-runtime`][metrics-runtime] to
[`metrics-util`][metrics-util] as a set of reusable components.

In doing so, though, we lost the ability to access metrics directly, as the [`metrics`][metrics] facade became
the only way to register or emit metrics, and this could only happen through the opaque macros,
going through the opaque `Recorder` trait, and so on.  This made it easy for all users to simply
interact with the macros, but made it harder to optimize the performance of recording metrics.

With this update, we've switched back to a handle-based model that also works seamlessly with the
macros.

#### Handle-based design

The `metrics` crate now provides concrete handle types for all metrics.  The `Recorder` API has also
been updated to return these handles when registering a metric.  Thus, for macros, instead of simply
calling the operation directly on the global recorder, they now register the metric first and call
the operation on the handle.  If the metric has already been registered, similar to as before, a
copy of the handle is simply returned and can be operated as normally.

On the flipside, though, users can use directly acquire these handles using the `register_*` macros.
The macros for adding descriptive information have been renamed to `describe_*`.  Thus, when there
is an opportunity to register metrics and store their handles somewhere for later reuse, users can
use the `register_*` macros to do so.

#### Implementation of handles

In order to provide an mechanism that allows enough flexibility for exporter authors, the design of the handle types was considered in depth.

Simply put, handles represent an incredibly thin layer over compatible implementations of each
metric type.  There are now three traits, one for each metric type, that must be implemented to
satisfy being wrapped in the corresponding handle type.  Internally, this is wrapped in an [`Arc<T>`][arc] to make it usable in multi-threaded scenarios.

Implementations of `Counter` and `Gauge` based on atomic integers can be found in
[`metrics`][metrics], and an example of `Histogram` can be found in [`metrics-util`][metrics-util].

#### Limitations of handles

In most cases, the new handle-based design will be effectively invisible, with no visible changes to
behavior.  There is one scenario where changes could be encountered, though: use of [layers][layers].

Layers work on a per-metric basis, which means that when a handle is acquired and used directly, any
layers that were used no longer have a way to intercept the calls made to handles.  When the macros
are used, however, a call is always made to "register" the metric, which provides a chance for the
layers to intercept and modify things as necessary.

As a concrete example, the [`metrics-tracing-context`][mtc] layer exists to annotate a metric's set of
labels with fields from the `tracing::Span` that is currently entered when the metric is updated.
Naturally, a given metric could be emitted while in a various number of different spans, but if a
metric is first registered in a particular span, and then the handle is used from that point on...
only the original span fields will be attached to the metric.

This is an inherent trade-off between providing a way to reduce the overhead of updating a metric to
the bare minimum, and providing extensibility via layers.

[metrics]: https://docs.rs/metrics/latest/metrics/
[metrics-runtime]: https://docs.rs/metrics-runtime/latest/metrics_runtime/
[metrics-util]: https://docs.rs/metrics-util/latest/metrics-util/
[arc]: https://doc.rust-lang.org/stable/std/sync/struct.Arc.html
[layers]: https://docs.rs/metrics-util/latest/metrics_util/layers/index.html
[mtc]: https://docs.rs/metrics-tracing-context/latest/metrics_tracing_context/
