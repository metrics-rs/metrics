//! Metric recency.
//!
//! Recency deals with the concept of removing metrics that have not been updated for a certain
//! amount of time.  In some use cases, metrics are tied to specific labels which are short-lived,
//! such as labels referencing a date or a version of software.  When these labels change, exporters
//! may still be emitting those older metrics which are no longer relevant.  In many cases, a
//! long-lived application could continue tracking metrics such that the unique number of metrics
//! grows until a significant portion of memory is required to track them all, even if the majority
//! of them are no longer used.
//!
//! This module contains the building blocks to both track recency and act on it.
//!
//! ## `Generation`, `Generational<T>`, and `GenerationalPrimitives`
//!
//! These three types form the basis of wrapping metrics so that they can be tracked with a
//! generation counter.  This counter is incremented every time a mutating operation is performed.
//! In tracking the generation of a metric, it can be determined whether or not a metric has changed
//! between two observations, even if the value of the metric is identical between the two
//! observations.
//!
//! ## `Recency`
//!
//! This type provides the tracking of metrics, and their generations, so that exporters can quickly
//! determine if a metric that has just been observed has been "idle" for a given amount of time or
//! longer.  This provides the final piece that allows exporters to remove metrics which are no
//! longer relevant to the application.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::{collections::HashMap, ops::DerefMut};

use crate::registry::Primitives;
use crate::StandardPrimitives;
use crate::{kind::MetricKindMask, MetricKind, Registry};

use metrics::{CounterFn, GaugeFn, HistogramFn, Key};
use parking_lot::Mutex;
use quanta::{Clock, Instant};

/// The generation of a metric.
///
/// Generations are opaque and are not meant to be used directly, but meant to be used as a
/// comparison amongst each other in terms of ordering.
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct Generation(usize);

/// Generation tracking for a metric.
///
/// Holds a generic interior value, and provides way to access the value such that each access
/// increments the "generation" of the value.  This provides a means to understand if the value has
/// been updated since the last time it was observed.
///
/// For example, if a gauge was observed to be X at one point in time, and then observed to be X
/// again at a later point in time, it could have changed in between the two observations.  It also
/// may not have changed, and thus `Generational` provides a way to determine if either of these
/// events occurred.
#[derive(Clone)]
pub struct Generational<T> {
    inner: T,
    gen: Arc<AtomicUsize>,
}

impl<T> Generational<T> {
    /// Creates a new `Generational<T>`.
    fn new(inner: T) -> Generational<T> {
        Generational {
            inner,
            gen: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Gets a reference to the inner value.
    pub fn get_inner(&self) -> &T {
        &self.inner
    }

    /// Gets the current generation.
    pub fn get_generation(&self) -> Generation {
        Generation(self.gen.load(Ordering::Acquire))
    }

    /// Acquires a reference to the inner value, and increments the generation.
    pub fn with_increment<F, V>(&self, f: F) -> V
    where
        F: Fn(&T) -> V,
    {
        let result = f(&self.inner);
        let _ = self.gen.fetch_add(1, Ordering::AcqRel);
        result
    }
}

impl<T> CounterFn for Generational<T>
where
    T: CounterFn,
{
    fn increment(&self, value: u64) {
        self.with_increment(|c| c.increment(value))
    }

    fn absolute(&self, value: u64) {
        self.with_increment(|c| c.absolute(value))
    }
}

impl<T> GaugeFn for Generational<T>
where
    T: GaugeFn,
{
    fn increment(&self, value: f64) {
        self.with_increment(|g| g.increment(value))
    }

    fn decrement(&self, value: f64) {
        self.with_increment(|g| g.decrement(value))
    }

    fn set(&self, value: f64) {
        self.with_increment(|g| g.set(value))
    }
}

impl<T> HistogramFn for Generational<T>
where
    T: HistogramFn,
{
    fn record(&self, value: f64) {
        self.with_increment(|h| h.record(value))
    }
}

/// Primitives for tracking the generation of metrics.
///
/// [`Generational<T>`] explains more about the purpose of generation tracking.
pub struct GenerationalPrimitives;

impl Primitives for GenerationalPrimitives {
    type Counter = Generational<<StandardPrimitives as Primitives>::Counter>;
    type Gauge = Generational<<StandardPrimitives as Primitives>::Gauge>;
    type Histogram = Generational<<StandardPrimitives as Primitives>::Histogram>;

    fn counter() -> Self::Counter {
        let counter = <StandardPrimitives as Primitives>::counter();
        Generational::new(counter)
    }

    fn gauge() -> Self::Gauge {
        let gauge = <StandardPrimitives as Primitives>::gauge();
        Generational::new(gauge)
    }

    fn histogram() -> Self::Histogram {
        let histogram = <StandardPrimitives as Primitives>::histogram();
        Generational::new(histogram)
    }
}

/// Tracks recency of metric updates by their registry generation and time.
///
/// In many cases, a user may have a long-running process where metrics are stored over time using
/// labels that change for some particular reason, leaving behind versions of that metric with
/// labels that are no longer relevant to the current process state.  This can lead to cases where
/// metrics that no longer matter are still present in rendered output, adding bloat.
///
/// When coupled with [`Registry`](crate::Registry), [`Recency`] can be used to track when the last
/// update to a metric has occurred for the purposes of removing idle metrics from the registry.  In
/// addition, it will remove the value from the registry itself to reduce the aforementioned bloat.
///
/// [`Recency`] is separate from [`Registry`](crate::Registry) specifically to avoid imposing any
/// slowdowns when tracking recency does not matter, despite their otherwise tight coupling.
pub struct Recency {
    mask: MetricKindMask,
    inner: Mutex<(Clock, HashMap<Key, (Generation, Instant)>)>,
    idle_timeout: Option<Duration>,
}

impl Recency {
    /// Creates a new [`Recency`].
    ///
    /// If `idle_timeout` is `None`, no recency checking will occur.  `mask` controls which metrics
    /// are covered by the recency logic.  For example, if `mask` only contains counters and
    /// histograms, then gauges will not be considered for recency, and thus will never be deleted.
    ///
    /// If `idle_timeout` is not `None`, then metrics which have not been updated within the given
    /// duration will be subject to deletion when checked.  Specifically, the deletions done by this
    /// object only happen when the object is "driven" by calling
    /// [`should_store`](Recency::should_store), and so handles will not necessarily be deleted
    /// immediately after execeeding their idle timeout.
    ///
    /// Refer to the documentation for [`MetricKindMask`](crate::MetricKindMask) for more
    /// information on defining a metric kind mask.
    pub fn new(clock: Clock, mask: MetricKindMask, idle_timeout: Option<Duration>) -> Recency {
        Recency {
            mask,
            inner: Mutex::new((clock, HashMap::new())),
            idle_timeout,
        }
    }

    /// Checks if the given counter should be stored, based on its known recency.
    ///
    /// If the given key has been updated recently enough, and should continue to be stored, this
    /// method will return `true` and will update the last update time internally.  If the given key
    /// has not been updated recently enough, the key will be removed from the given registry if the
    /// given generation also matches.
    pub fn should_store_counter(
        &self,
        key: &Key,
        gen: Generation,
        registry: &Registry<GenerationalPrimitives>,
    ) -> bool {
        self.should_store(key, gen, registry, MetricKind::Counter, |registry, key| {
            registry.delete_counter(key)
        })
    }

    /// Checks if the given gauge should be stored, based on its known recency.
    ///
    /// If the given key has been updated recently enough, and should continue to be stored, this
    /// method will return `true` and will update the last update time internally.  If the given key
    /// has not been updated recently enough, the key will be removed from the given registry if the
    /// given generation also matches.
    pub fn should_store_gauge(
        &self,
        key: &Key,
        gen: Generation,
        registry: &Registry<GenerationalPrimitives>,
    ) -> bool {
        self.should_store(key, gen, registry, MetricKind::Gauge, |registry, key| {
            registry.delete_gauge(key)
        })
    }

    /// Checks if the given histogram should be stored, based on its known recency.
    ///
    /// If the given key has been updated recently enough, and should continue to be stored, this
    /// method will return `true` and will update the last update time internally.  If the given key
    /// has not been updated recently enough, the key will be removed from the given registry if the
    /// given generation also matches.
    pub fn should_store_histogram(
        &self,
        key: &Key,
        gen: Generation,
        registry: &Registry<GenerationalPrimitives>,
    ) -> bool {
        self.should_store(
            key,
            gen,
            registry,
            MetricKind::Histogram,
            |registry, key| registry.delete_histogram(key),
        )
    }

    fn should_store<F>(
        &self,
        key: &Key,
        gen: Generation,
        registry: &Registry<GenerationalPrimitives>,
        kind: MetricKind,
        delete_op: F,
    ) -> bool
    where
        F: Fn(&Registry<GenerationalPrimitives>, &Key) -> bool,
    {
        if let Some(idle_timeout) = self.idle_timeout {
            if self.mask.matches(kind) {
                let mut guard = self.inner.lock();
                let (clock, entries) = guard.deref_mut();

                let now = clock.now();
                if let Some((last_gen, last_update)) = entries.get_mut(key) {
                    // If the value is the same as the latest value we have internally, and
                    // we're over the idle timeout period, then remove it and continue.
                    if *last_gen == gen {
                        if (now - *last_update) > idle_timeout {
                            // If the delete returns false, that means that our generation counter is
                            // out-of-date, and that the metric has been updated since, so we don't
                            // actually want to delete it yet.
                            if delete_op(registry, key) {
                                return false;
                            }
                        }
                    } else {
                        // Value has changed, so mark it such.
                        *last_update = now;
                    }
                } else {
                    entries.insert(key.clone(), (gen, now));
                }
            }
        }

        true
    }
}
