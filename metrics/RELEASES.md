# Releases

Unlike the [CHANGELOG](CHANGELOG.md), this file tracks more complicated changes that required
long-form description and would be too verbose for the changelog alone.

<!-- next-header -->

## [Unreleased] - ReleaseDate

- No notable changes.

## [0.22.3] - 2024-03-16

- No notable changes.

## [0.22.2] - 2024-03-16

- No notable changes.

## [0.22.1] - 2024-02-11

- No notable changes.

## [0.22.0] - 2023-12-24

### Metric metadata

Metrics now support collecting a limited set of metadata field, which can be provided to add context
on where a metric originates from as well as its verbosity.

In the grand vision of the `metrics` crate, where library authors use it to emit metrics from their
libraries, and then downstream users get those metrics in their application for free... the biggest
problem is that users had no way to actually filter out metrics they didn't want. Metric metadata
aims to provide a solution to this problem.

A new type `Metadata<'a>` has been added to track all of this information, which includes **module
path**, a **target**, and a **level**. These fields map almost directly to
[`tracing`](https://docs.rs/tracing) -- the inspiration for adding this metadata support -- and
provide the ability to:

- group/filter metrics by where they're defined (module path)
- group/filter metrics by where they're emitted from (target)
- group/filter metrics by their verbosity (level)

`Metadata<'a>` is passed into the `Recorder` API when registering a metric so that exporters can
capture it and utilize it.

#### Examples

As an example, users may wish to filter out metrics defined by a particular crate because they don't
care about them at all. While they might have previously been able to get lucky and simply filter
the metrics by a common prefix, this still allows for changes to the metric names to break the
filter configuration. If we instead filtered by module path, where we can simply use the crate name
itself, then we'd catch all metrics for that crate regardless of their name and regardless of the
crate version.

Similarly, as another example, users may wish to only emit common metrics related to operation of
their application/service in order to consume less resources, pay less money for the ingest/storage
of the metrics, and so on. During an outage, or when debugging an issue, though, they may wish to
increase the verbosity of metrics they emit in order to capture more granular detail. Being able to
filter by level now provides a mechanism to do so.

#### Usage

First, it needs to be said that nothing in the core `metrics` crates actually utilizes this
metadata yet. We'll add support in the future to existing layers, such as the
[filter][filter_layer_docs] layer, in order to take advantage of this support.

With that said, actually setting this metadata is very easy! As a refresher, you'd normally emit
metrics with something like this:

```rust
metrics::increment_counter!("my_counter");
```

Now, you can specify the additional metadata attributes as fields at the beginning of the macro
call. This applies to all of the "emission" macros for counters, gauges, and histograms:

```rust
metrics::increment_counter!(target: "myapp", "my_counter");

metrics::increment_gauge!(level: metrics::Level::DEBUG, "my_gauge", 42.2);

metrics::histogram!(target: "myapp", level: metrics::Level::DEBUG, "my_histogram", 180.1);
```

These metrics will have the relevant metadata field set, and all of them will get the module path
provided automatically, as well.

### Macros overhaul

We've reworked the macros to both simplify their implementation and to hopefully provide a more
ergonomic experience for users.

At a high level, we've:

- removed all the various macros that were tied to specific _operations_ (e.g. `increment_counter!`
  for incrementing a counter) and replaced them with one macro per metric type
- removed the standalone registration macros (e.g. `register_counter!`)
- exposed the operations as methods on the metric handles themselves
- switched from using procedural macros to declarative macros

#### Fewer macros, more ergonomic usage

Users no longer need to remember the specific macro to use for a given metric operation, such as
`increment_gauge!` or `decrement_gauge!`. Instead, if the user knows they're working with a gauge,
they simply call `gauge!(...)` to get the handle, and chain on a method call to perform the
operation, such as `gauge!(...).increment(42.2)`.

Additionally, because we've condensed the registration macros into the new, simplified macros, the
same macro is used whether registering the metric to get a handle, or simply performing an operation
on the metric all at once.

Let's look at a few examples:

```rust
// This is the old style of registering a metric and then performing an operation on it.
//
// We're working with a counter here.
let counter_handle = metrics::register_counter!("my_counter");
counter_handle.increment(1);

metrics::increment_counter!("my_counter");

// Now let's use the new, simplified macros instead:
let counter_handle = metrics::counter!("my_counter");
counter_handle.increment(1);

metrics::counter!("my_counter").increment(1);
```

As you can see, users no longer need to know about as many macros, and their usage is consistent
whether working with a metric handle that is held long-term, or chaining the method call inline with
the macro call. As a benefit, this also means that IDE completion will be better in some situations,
as support for autocompletion of method calls is generally well supported, while macro
autocompletion is effectively nonexistent.

#### Declarative macros

As part of this rework, the macros have also been rewritten declaratively. While the macro code
itself is slightly more complicated/verbose, it has a major benefit that the `metrics-macros` crate
was able to be removed. This is one less dependency that has to be compiled, which should hopefully
help with build times, even if only slightly.

[filter_layer_docs]: https://docs.rs/metrics-util/latest/metrics_util/layers/struct.FilterLayer.html

### Scoped recorders

We've added support for scoped recorders, which should allow both library authors writing exporters,
as well as downstream users of `metrics`, test their implementations far more easily.

#### Global recorder

Prior to this release, the only way to test either exporters or metrics emission was to install a
special debug/test recorder as the global recorder. This conceptually works, but quickly runs into a
few issues:

- it's not thread-safe (the global recorder is, well, global)
- it requires the recorder be _implemented_ in a thread-safe way

This meant that in order to safely do this type of testing, users would have to use something like
[`DebuggingRecorder`](https://docs.rs/metrics-util/latest/metrics_util/debugging/struct.DebuggingRecorder.html)
(which is thread-safe and _could_ be used in a per-thread way, mind you) but that they would have to
install it for every single test... which could still run into issues with destroying the collected
metrics of another concurrently running test that took the same approach.

All in all, this was a pretty poor experience that required many compromises and _still_ didn't
fully allow testing metrics in a deterministic and repeatable way.

#### Scoped recorders

Scoped recorders solve this problem by allowing the temporary overriding of what is considered the
"global" recorder on the current thread. We've added a new method,
[`metrics::with_local_recorder`](https://docs.rs/metrics/latest/metrics/fn.set_boxed_recorder.html),
that allows users to pass a reference to a recorder that is used as the "global" recorder for the
duration of a closure that the user also passes.

Here's a quick example of what the prior approach looked like, and how it's been simplified by
adding support for scoped recorders:

```rust
// This is the old approach, which mind you, still isn't thread-safe if you have concurrent tests
// doing the same thing:
let global = DebuggingRecorder::per_thread();
let snapshotter = global.snapshotter();

unsafe { metrics::clear_recorder(); }
global.install().expect("global recorder should not be installed");

increment_counter!("my_counter");

let snapshot = snapshotter.snapshot();
assert_eq!(snapshot.into_vec().len(), 1);

// Now let's use a scoped recorder instead:
let scoped = DebuggingRecorder::new();
let snapshotter = scoped.snappshotter();

metrics::with_local_recorder(&scoped, || {
    increment_counter!("my_counter")
});

let snapshot = snapppshotter.snapshot();
assert_eq!(snapshot.into_vec().len(), 1);
```

There's a lot of boilerplate here, but let's look specifically at what we _don't_ have to do
anymore:

- unsafely clear the global recorder
- install as the global recorder
- transfer ownership of the recorder itself

This means that recorders can now themselves hold references to other resources, without the need to
do the common `Arc<Mutex<...>>` dance that was previously required. Given the interface of
`Recorder`, interior mutability still has to be provided somehow, although now it only requires the
use of something as straightforward as `RefCell`.

Beyond testing, this also opens up additional functionality that wasn't previously available. For
example, this approach could be used to avoid the need for using thread-safe primitives when users
don't want to pay that cost (perhaps on an embedded system).

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
