//! Metric recency.
//!
//! `Recency` deals with the concept of removing metrics that have not been updated for a certain
//! amount of time.  In some use cases, metrics are tied to specific labels which are short-lived,
//! such as labels referencing a date or a version of software.  When these labels change, exporters
//! may still be emitting those older metrics which are no longer relevant.  In many cases, a
//! long-lived application could continue tracking metrics such that the unique number of metrics
//! grows until a significant portion of memory is required to track them all, even if the majority
//! of them are no longer used.
//!
//! As metrics are typically backed by atomic storage, exporters don't see the individual changes to
//! a metric, and so need a way to measure if a metric has changed since the last time it was
//! observed.  This could potentially be achieved by observing the value directly, but metrics like
//! gauges can be updated in such a way that their value is the same between two observations even
//! though it had actually been changed in between.
//!
//! We solve for this by tracking the generation of a metric, which represents the number of times
//! it has been modified. In doing so, we can compare the generation of a metric between
//! observations, which only ever increases monotonically.  This provides a universal mechanism that
//! works for all metric types.
//!
//! `Recency` uses the generation of a metric, along with a measurement of time when a metric is
//! observed, to build a complete picture that allows deciding if a given metric has gone "idle" or
//! not, and thus whether it should actually be deleted.
//!
//! Idleness alone is not sufficient to delete a metric, however, as the caller may still be holding
//! the handle it was given when the metric was registered.  Deleting the registry's entry for such
//! a metric frees nothing, since the handle owns the storage, and it orphans the holder: its
//! subsequent updates land in storage the registry can no longer reach, and the metric is silent
//! for good.  `Recency` therefore only deletes metrics that the registry holds the last reference
//! to.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use std::{collections::HashMap, ops::DerefMut};

use metrics::{Counter, CounterFn, Gauge, GaugeFn, Histogram, HistogramFn};
use quanta::{Clock, Instant};

use crate::Hashable;
use crate::{
    kind::MetricKindMask,
    registry::{AtomicStorage, Registry, Storage},
    MetricKind,
};

/// The generation of a metric.
///
/// Generations are opaque and are not meant to be used directly, but meant to be used as a
/// comparison amongst each other in terms of ordering.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
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
#[derive(Clone, Debug)]
pub struct Generational<T> {
    inner: T,
    gen: Arc<AtomicUsize>,
}

impl<T> Generational<T> {
    /// Creates a new `Generational<T>`.
    fn new(inner: T) -> Generational<T> {
        Generational { inner, gen: Arc::new(AtomicUsize::new(0)) }
    }

    /// Gets a reference to the inner value.
    pub fn get_inner(&self) -> &T {
        &self.inner
    }

    /// Gets the current generation.
    pub fn get_generation(&self) -> Generation {
        Generation(self.gen.load(Ordering::Acquire))
    }

    /// Whether anything other than the registry is holding this metric.
    ///
    /// Every clone of a `Generational` clones `gen`, and a handle handed out at registration owns
    /// one such clone, so a strong count above one means the metric has a live holder.  This is
    /// only meaningful from inside [`Registry::retain_counters`] and friends, where the map's own
    /// copy is the sole reference the registry owns, and where holding the shard lock means no
    /// further clone can be handed out while we decide.
    fn is_held(&self) -> bool {
        Arc::strong_count(&self.gen) > 1
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

impl<T> From<Generational<T>> for Counter
where
    T: CounterFn + Send + Sync + 'static,
{
    fn from(inner: Generational<T>) -> Self {
        Counter::from_arc(Arc::new(inner))
    }
}

impl<T> From<Generational<T>> for Gauge
where
    T: GaugeFn + Send + Sync + 'static,
{
    fn from(inner: Generational<T>) -> Self {
        Gauge::from_arc(Arc::new(inner))
    }
}

impl<T> From<Generational<T>> for Histogram
where
    T: HistogramFn + Send + Sync + 'static,
{
    fn from(inner: Generational<T>) -> Self {
        Histogram::from_arc(Arc::new(inner))
    }
}

/// Generational metric storage.
///
/// Tracks the "generation" of a metric, which is used to detect updates to metrics where the value
/// otherwise would not be sufficient to be used as an indicator.
#[derive(Debug)]
pub struct GenerationalStorage<S> {
    inner: S,
}

impl<S> GenerationalStorage<S> {
    /// Creates a new [`GenerationalStorage`].
    ///
    /// This wraps the given `storage` and provides generational semantics on top of it.
    pub fn new(storage: S) -> Self {
        Self { inner: storage }
    }
}

impl<K, S: Storage<K>> Storage<K> for GenerationalStorage<S> {
    type Counter = Generational<S::Counter>;
    type Gauge = Generational<S::Gauge>;
    type Histogram = Generational<S::Histogram>;

    fn counter(&self, key: &K) -> Self::Counter {
        Generational::new(self.inner.counter(key))
    }

    fn gauge(&self, key: &K) -> Self::Gauge {
        Generational::new(self.inner.gauge(key))
    }

    fn histogram(&self, key: &K) -> Self::Histogram {
        Generational::new(self.inner.histogram(key))
    }
}

/// Generational atomic metric storage.
///
/// `GenerationalAtomicStorage` is based on [`AtomicStorage`], but additionally tracks the
/// "generation" of a metric, which is used to detect updates to metrics where the value otherwise
/// would not be sufficient to be used as an indicator.
pub type GenerationalAtomicStorage = GenerationalStorage<AtomicStorage>;

impl GenerationalAtomicStorage {
    /// Creates a [`GenerationalStorage`] that uses [`AtomicStorage`] as its underlying storage.
    pub fn atomic() -> Self {
        Self { inner: AtomicStorage }
    }
}

/// Tracks recency of metric updates by their registry generation and time.
///
/// In many cases, a user may have a long-running process where metrics are stored over time using
/// labels that change for some particular reason, leaving behind versions of that metric with
/// labels that are no longer relevant to the current process state.  This can lead to cases where
/// metrics that no longer matter are still present in rendered output, adding bloat.
///
/// When coupled with [`Registry`], [`Recency`] can be used to track when the last update to a
/// metric has occurred for the purposes of removing idle metrics from the registry.  In addition,
/// it will remove the value from the registry itself to reduce the aforementioned bloat.
///
/// Metrics whose handles are still held outside of the registry are never removed, however idle
/// they are: the holder can still write to such a metric, and removing it would silence those
/// writes while freeing nothing.  Such a metric is removed by the first sweep after its last
/// handle is dropped, without being granted a fresh idle period.
///
/// [`Recency`] is separate from [`Registry`] specifically to avoid imposing any slowdowns when
/// tracking recency does not matter, despite their otherwise tight coupling.
#[derive(Debug)]
pub struct Recency<K> {
    mask: MetricKindMask,
    #[allow(clippy::type_complexity)]
    inner: Mutex<(Clock, HashMap<K, (Generation, Instant)>)>,
    idle_timeout: Option<Duration>,
}

impl<K> Recency<K>
where
    K: Clone + Eq + Hashable,
{
    /// Creates a new [`Recency`].
    ///
    /// If `idle_timeout` is `None`, no recency checking will occur.  Otherwise, any metric that has
    /// not been updated for longer than `idle_timeout` will be subject for deletion the next time
    /// the metric is checked.
    ///
    /// The provided `clock` is used for tracking time, while `mask` controls which metrics
    /// are covered by the recency logic.  For example, if `mask` only contains counters and
    /// histograms, then gauges will not be considered for recency, and thus will never be deleted.
    ///
    /// Refer to the documentation for [`MetricKindMask`](crate::MetricKindMask) for more
    /// information on defining a metric kind mask.
    pub fn new(clock: Clock, mask: MetricKindMask, idle_timeout: Option<Duration>) -> Self {
        Recency { mask, inner: Mutex::new((clock, HashMap::new())), idle_timeout }
    }

    /// Removes idle counters from the given registry, returning the keys that were removed.
    ///
    /// A counter is removed if it has not been updated within the idle timeout and the registry
    /// holds the last reference to it.  A counter the application is still holding a handle to is
    /// kept, however idle, and is removed by the first call made after that handle is dropped.
    pub fn evict_idle_counters<S>(&self, registry: &Registry<K, GenerationalStorage<S>>) -> Vec<K>
    where
        S: Storage<K>,
    {
        self.evict(MetricKind::Counter, |f| registry.retain_counters(f))
    }

    /// Removes idle gauges from the given registry, returning the keys that were removed.
    ///
    /// A gauge is removed if it has not been updated within the idle timeout and the registry holds
    /// the last reference to it.  A gauge the application is still holding a handle to is kept,
    /// however idle, and is removed by the first call made after that handle is dropped.
    pub fn evict_idle_gauges<S>(&self, registry: &Registry<K, GenerationalStorage<S>>) -> Vec<K>
    where
        S: Storage<K>,
    {
        self.evict(MetricKind::Gauge, |f| registry.retain_gauges(f))
    }

    /// Removes idle histograms from the given registry, returning the keys that were removed.
    ///
    /// A histogram is removed if it has not been updated within the idle timeout and the registry
    /// holds the last reference to it.  A histogram the application is still holding a handle to is
    /// kept, however idle, and is removed by the first call made after that handle is dropped.
    ///
    /// Exporters that keep their own state per metric, such as aggregated distributions, should use
    /// the returned keys to drop the state belonging to the removed histograms.
    pub fn evict_idle_histograms<S>(&self, registry: &Registry<K, GenerationalStorage<S>>) -> Vec<K>
    where
        S: Storage<K>,
    {
        self.evict(MetricKind::Histogram, |f| registry.retain_histograms(f))
    }

    fn evict<T, R>(&self, kind: MetricKind, retain: R) -> Vec<K>
    where
        R: FnOnce(&mut dyn FnMut(&K, &Generational<T>) -> bool),
    {
        let mut evicted = Vec::new();

        let idle_timeout = match self.idle_timeout {
            Some(idle_timeout) if self.mask.matches(kind) => idle_timeout,
            _ => return evicted,
        };

        let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let (clock, entries) = guard.deref_mut();
        let now = clock.now();

        // Deciding from inside `retain` is what makes `is_held` sound: the shard write lock is held
        // for the whole visit, and handles are only ever cloned out of the registry under that same
        // lock, so no holder can appear between the check and the removal.
        retain(&mut |key, handle| {
            let gen = handle.get_generation();
            let evict = if let Some((last_gen, last_update)) = entries.get_mut(key) {
                // If the value is the same as the latest value we have internally, and we're over
                // the idle timeout period, then remove it, unless somebody still holds it.
                if *last_gen == gen {
                    (now - *last_update) > idle_timeout && !handle.is_held()
                } else {
                    // Value has changed, so mark it such.
                    *last_update = now;
                    *last_gen = gen;
                    false
                }
            } else {
                entries.insert(key.clone(), (gen, now));
                false
            };

            if evict {
                entries.remove(key);
                evicted.push(key.clone());
            }

            !evict
        });

        evicted
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use metrics::{Counter, CounterFn, Gauge, Histogram, Key};
    use quanta::Clock;

    use super::{GenerationalStorage, Recency};
    use crate::{registry::Registry, MetricKindMask};

    fn recency(clock: Clock, mask: MetricKindMask) -> Recency<Key> {
        Recency::new(clock, mask, Some(Duration::from_secs(10)))
    }

    #[test]
    fn evicts_idle_counters_but_not_held_ones() {
        let (clock, mock) = Clock::mock();
        let registry = Registry::new(GenerationalStorage::atomic());
        let recency = recency(clock, MetricKindMask::ALL);

        let held = Key::from_name("held");
        let transient = Key::from_name("transient");
        let handle: Counter = registry.get_or_create_counter(&held, |c| c.clone().into());
        handle.increment(1);
        let _: Counter = registry.get_or_create_counter(&transient, |c| c.clone().into());

        // The first sweep only records when each metric was last seen.
        assert!(recency.evict_idle_counters(&registry).is_empty());

        mock.increment(Duration::from_secs(11));
        assert_eq!(recency.evict_idle_counters(&registry), vec![transient.clone()]);
        assert!(registry.get_counter(&transient).is_none());
        assert!(registry.get_counter(&held).is_some(), "still held by the caller");

        // Dropping the last handle makes it evictable right away: being held does not grant a
        // metric a fresh idle period.
        drop(handle);
        assert_eq!(recency.evict_idle_counters(&registry), vec![held.clone()]);
        assert!(registry.get_counter(&held).is_none());
    }

    #[test]
    fn evicts_idle_histograms_and_returns_their_keys() {
        let (clock, mock) = Clock::mock();
        let registry = Registry::new(GenerationalStorage::atomic());
        let recency = recency(clock, MetricKindMask::ALL);

        let key = Key::from_name("histogram");
        let handle: Histogram = registry.get_or_create_histogram(&key, |h| h.clone().into());
        handle.record(1.0);

        assert!(recency.evict_idle_histograms(&registry).is_empty());

        mock.increment(Duration::from_secs(11));
        assert!(recency.evict_idle_histograms(&registry).is_empty(), "still held by the caller");

        drop(handle);
        assert_eq!(recency.evict_idle_histograms(&registry), vec![key.clone()]);
        assert!(registry.get_histogram(&key).is_none());
    }

    #[test]
    fn keeps_metrics_that_are_still_being_updated() {
        let (clock, mock) = Clock::mock();
        let registry = Registry::new(GenerationalStorage::atomic());
        let recency = recency(clock, MetricKindMask::ALL);

        let key = Key::from_name("busy");
        registry.get_or_create_counter(&key, |c| c.increment(1));

        for _ in 0..3 {
            mock.increment(Duration::from_secs(11));
            registry.get_or_create_counter(&key, |c| c.increment(1));
            assert!(recency.evict_idle_counters(&registry).is_empty());
        }

        mock.increment(Duration::from_secs(11));
        assert_eq!(recency.evict_idle_counters(&registry), vec![key]);
    }

    #[test]
    fn ignores_metrics_outside_the_mask() {
        let (clock, mock) = Clock::mock();
        let registry = Registry::new(GenerationalStorage::atomic());
        let recency = recency(clock, MetricKindMask::COUNTER);

        let key = Key::from_name("gauge");
        let _: Gauge = registry.get_or_create_gauge(&key, |g| g.clone().into());

        assert!(recency.evict_idle_gauges(&registry).is_empty());
        mock.increment(Duration::from_secs(11));
        assert!(recency.evict_idle_gauges(&registry).is_empty());
        assert!(registry.get_gauge(&key).is_some());
    }
}
