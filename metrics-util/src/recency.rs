use std::fmt::Debug;
use std::time::Duration;
use std::{collections::HashMap, ops::DerefMut};

use crate::registry::GenerationalFamily;
use crate::{kind::MetricKindMask, Generation, MetricKind, Registry};

use metrics::Key;
use parking_lot::Mutex;
use quanta::{Clock, Instant};

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
#[derive(Debug)]
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
    ///
    /// If the generation does not match, this indicates that the key was updated between querying
    /// it from the registry and calling this method, and this method will return `true` in those
    /// cases, and `false` for all remaining cases.
    pub fn should_store_counter<G>(
        &self,
        key: &Key,
        gen: Generation,
        registry: &Registry<G>,
    ) -> bool
    where
        G: GenerationalFamily,
    {
        self.should_store(key, gen, registry, MetricKind::Counter, |registry, key, gen| {
            registry.delete_counter_with_gen(key, gen)
        })
    }

    /// Checks if the given gauge should be stored, based on its known recency.
    ///
    /// If the given key has been updated recently enough, and should continue to be stored, this
    /// method will return `true` and will update the last update time internally.  If the given key
    /// has not been updated recently enough, the key will be removed from the given registry if the
    /// given generation also matches.
    ///
    /// If the generation does not match, this indicates that the key was updated between querying
    /// it from the registry and calling this method, and this method will return `true` in those
    /// cases, and `false` for all remaining cases.
    pub fn should_store_gauge<G>(
        &self,
        key: &Key,
        gen: Generation,
        registry: &Registry<G>,
    ) -> bool
    where
        G: GenerationalFamily,
    {
        self.should_store(key, gen, registry, MetricKind::Gauge, |registry, key, gen| {
            registry.delete_gauge_with_gen(key, gen)
        })
    }

    /// Checks if the given histogram should be stored, based on its known recency.
    ///
    /// If the given key has been updated recently enough, and should continue to be stored, this
    /// method will return `true` and will update the last update time internally.  If the given key
    /// has not been updated recently enough, the key will be removed from the given registry if the
    /// given generation also matches.
    ///
    /// If the generation does not match, this indicates that the key was updated between querying
    /// it from the registry and calling this method, and this method will return `true` in those
    /// cases, and `false` for all remaining cases.
    pub fn should_store_histogram<G>(
        &self,
        key: &Key,
        gen: Generation,
        registry: &Registry<G>,
    ) -> bool
    where
        G: GenerationalFamily,
    {
        self.should_store(key, gen, registry, MetricKind::Histogram, |registry, key, gen| {
            registry.delete_histogram_with_gen(key, gen)
        })
    }

    fn should_store<F, G>(&self, key: &Key, gen: Generation, registry: &Registry<G>, kind: MetricKind, delete_op: F) -> bool
    where
        F: Fn(&Registry<G>, &Key, Generation) -> bool,
        G: GenerationalFamily,
    {
        if let Some(idle_timeout) = self.idle_timeout {
            if self.mask.matches(kind) {
                let mut guard = self.inner.lock();
                let (clock, entries) = guard.deref_mut();

                let now = clock.now();
                if let Some((last_gen, last_update)) = entries.get_mut(&key) {
                    // If the value is the same as the latest value we have internally, and
                    // we're over the idle timeout period, then remove it and continue.
                    if *last_gen == gen {
                        if (now - *last_update) > idle_timeout {
                            // If the delete returns false, that means that our generation counter is
                            // out-of-date, and that the metric has been updated since, so we don't
                            // actually want to delete it yet.
                            if delete_op(registry, key, gen) {
                                return true
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
