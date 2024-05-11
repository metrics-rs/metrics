//! High-performance metrics storage.

mod storage;
use std::{
    hash::BuildHasherDefault,
    iter::repeat,
    sync::{PoisonError, RwLock},
};

use hashbrown::{hash_map::RawEntryMut, HashMap};
use metrics::{Key, KeyHasher};
pub use storage::{AtomicStorage, Storage};

#[cfg(feature = "recency")]
mod recency;

#[cfg(feature = "recency")]
#[cfg_attr(docsrs, doc(cfg(feature = "recency")))]
pub use recency::{
    Generation, Generational, GenerationalAtomicStorage, GenerationalStorage, Recency,
};

use crate::Hashable;

type RegistryHasher = KeyHasher;
type RegistryHashMap<K, V> = HashMap<K, V, BuildHasherDefault<RegistryHasher>>;

/// A high-performance metric registry.
///
/// `Registry` provides the ability to maintain a central listing of metrics mapped by a given key.
/// Metrics themselves are stored in the objects returned by `S`.
///
/// ## Using `Registry` as the basis of an exporter
///
/// As a reusable building blocking for building exporter implementations, users should look at
/// [`Key`] and [`AtomicStorage`] to use for their key and storage, respectively.
///
/// These two implementations provide behavior that is suitable for most exporters, providing
/// seamless integration with the existing key type used by the core
/// [`Recorder`][metrics::Recorder] trait, as well as atomic storage for metrics.
///
/// In some cases, users may prefer [`GenerationalAtomicStorage`] when know if a metric has been
/// touched, even if its value has not changed since the last time it was observed, is necessary.
///
/// ## Performance
///
/// `Registry` is optimized for reads.
pub struct Registry<K, S>
where
    S: Storage<K>,
{
    counters: Vec<RwLock<RegistryHashMap<K, S::Counter>>>,
    gauges: Vec<RwLock<RegistryHashMap<K, S::Gauge>>>,
    histograms: Vec<RwLock<RegistryHashMap<K, S::Histogram>>>,
    shard_mask: usize,
    storage: S,
}

impl Registry<Key, AtomicStorage> {
    /// Creates a new `Registry` using a regular [`Key`] and atomic storage.
    pub fn atomic() -> Self {
        let shard_count = std::cmp::max(1, num_cpus::get()).next_power_of_two();
        let shard_mask = shard_count - 1;
        let counters =
            repeat(()).take(shard_count).map(|_| RwLock::new(RegistryHashMap::default())).collect();
        let gauges =
            repeat(()).take(shard_count).map(|_| RwLock::new(RegistryHashMap::default())).collect();
        let histograms =
            repeat(()).take(shard_count).map(|_| RwLock::new(RegistryHashMap::default())).collect();

        Self { counters, gauges, histograms, shard_mask, storage: AtomicStorage }
    }
}

impl<K, S> Registry<K, S>
where
    S: Storage<K>,
{
    /// Creates a new `Registry`.
    pub fn new(storage: S) -> Self {
        let shard_count = std::cmp::max(1, num_cpus::get()).next_power_of_two();
        let shard_mask = shard_count - 1;
        let counters =
            repeat(()).take(shard_count).map(|_| RwLock::new(RegistryHashMap::default())).collect();
        let gauges =
            repeat(()).take(shard_count).map(|_| RwLock::new(RegistryHashMap::default())).collect();
        let histograms =
            repeat(()).take(shard_count).map(|_| RwLock::new(RegistryHashMap::default())).collect();

        Self { counters, gauges, histograms, shard_mask, storage }
    }

    /// Removes all metrics from the registry.
    ///
    /// This operation is eventually consistent: metrics will be removed piecemeal, and this method
    /// does not ensure that callers will see the registry as entirely empty at any given point.
    pub fn clear(&self) {
        for shard in &self.counters {
            shard.write().unwrap_or_else(PoisonError::into_inner).clear();
        }
        for shard in &self.gauges {
            shard.write().unwrap_or_else(PoisonError::into_inner).clear();
        }
        for shard in &self.histograms {
            shard.write().unwrap_or_else(PoisonError::into_inner).clear();
        }
    }

    /// Visits every counter stored in this registry.
    ///
    /// This operation does not lock the entire registry, but proceeds directly through the
    /// "subshards" that are kept internally.  As a result, all subshards will be visited, but a
    /// metric that existed at the exact moment that `visit_counters` was called may not actually be observed
    /// if it is deleted before that subshard is reached.  Likewise, a metric that is added after
    /// the call to `visit_counters`, but before `visit_counters` finishes, may also not be observed.
    pub fn visit_counters<F>(&self, mut collect: F)
    where
        F: FnMut(&K, &S::Counter),
    {
        for subshard in self.counters.iter() {
            let shard_read = subshard.read().unwrap_or_else(PoisonError::into_inner);
            for (key, counter) in shard_read.iter() {
                collect(key, counter);
            }
        }
    }
    /// Visits every gauge stored in this registry.
    ///
    /// This operation does not lock the entire registry, but proceeds directly through the
    /// "subshards" that are kept internally.  As a result, all subshards will be visited, but a
    /// metric that existed at the exact moment that `visit_gauges` was called may not actually be observed
    /// if it is deleted before that subshard is reached.  Likewise, a metric that is added after
    /// the call to `visit_gauges`, but before `visit_gauges` finishes, may also not be observed.
    pub fn visit_gauges<F>(&self, mut collect: F)
    where
        F: FnMut(&K, &S::Gauge),
    {
        for subshard in self.gauges.iter() {
            let shard_read = subshard.read().unwrap_or_else(PoisonError::into_inner);
            for (key, gauge) in shard_read.iter() {
                collect(key, gauge);
            }
        }
    }

    /// Visits every histogram stored in this registry.
    ///
    /// This operation does not lock the entire registry, but proceeds directly through the
    /// "subshards" that are kept internally.  As a result, all subshards will be visited, but a
    /// metric that existed at the exact moment that `visit_histograms` was called may not actually be observed
    /// if it is deleted before that subshard is reached.  Likewise, a metric that is added after
    /// the call to `visit_histograms`, but before `visit_histograms` finishes, may also not be observed.
    pub fn visit_histograms<F>(&self, mut collect: F)
    where
        F: FnMut(&K, &S::Histogram),
    {
        for subshard in self.histograms.iter() {
            let shard_read = subshard.read().unwrap_or_else(PoisonError::into_inner);
            for (key, histogram) in shard_read.iter() {
                collect(key, histogram);
            }
        }
    }

    /// Retains only counters specified by the predicate.
    ///
    /// Remove all counters for which f(&k, &c) returns false. This operation proceeds
    /// through the "subshards" in the same way as `visit_counters`.
    pub fn retain_counters<F>(&self, mut f: F)
    where
        F: FnMut(&K, &S::Counter) -> bool,
    {
        for subshard in self.counters.iter() {
            let mut shard_write = subshard.write().unwrap_or_else(PoisonError::into_inner);
            shard_write.retain(|k, c| f(k, c));
        }
    }

    /// Retains only gauges specified by the predicate.
    ///
    /// Remove all gauges for which f(&k, &g) returns false. This operation proceeds
    /// through the "subshards" in the same way as `visit_gauges`.
    pub fn retain_gauges<F>(&self, mut f: F)
    where
        F: FnMut(&K, &S::Gauge) -> bool,
    {
        for subshard in self.gauges.iter() {
            let mut shard_write = subshard.write().unwrap_or_else(PoisonError::into_inner);
            shard_write.retain(|k, g| f(k, g));
        }
    }

    /// Retains only histograms specified by the predicate.
    ///
    /// Remove all histograms for which f(&k, &h) returns false. This operation proceeds
    /// through the "subshards" in the same way as `visit_histograms`.
    pub fn retain_histograms<F>(&self, mut f: F)
    where
        F: FnMut(&K, &S::Histogram) -> bool,
    {
        for subshard in self.histograms.iter() {
            let mut shard_write = subshard.write().unwrap_or_else(PoisonError::into_inner);
            shard_write.retain(|k, h| f(k, h));
        }
    }
}

impl<K, S> Registry<K, S>
where
    S: Storage<K>,
    K: Hashable,
{
    #[inline]
    fn get_hash_and_shard_for_counter(
        &self,
        key: &K,
    ) -> (u64, &RwLock<RegistryHashMap<K, S::Counter>>) {
        let hash = key.hashable();

        // SAFETY: We initialize vector of subshards with a power-of-two value, and
        // `self.shard_mask` is `self.counters.len() - 1`, thus we can never have a result from the
        // masking operation that results in a value which is not in bounds of our subshards vector.
        let shard = unsafe { self.counters.get_unchecked(hash as usize & self.shard_mask) };

        (hash, shard)
    }

    #[inline]
    fn get_hash_and_shard_for_gauge(
        &self,
        key: &K,
    ) -> (u64, &RwLock<RegistryHashMap<K, S::Gauge>>) {
        let hash = key.hashable();

        // SAFETY: We initialize the vector of subshards with a power-of-two value, and
        // `self.shard_mask` is `self.gauges.len() - 1`, thus we can never have a result from the
        // masking operation that results in a value which is not in bounds of our subshards vector.
        let shard = unsafe { self.gauges.get_unchecked(hash as usize & self.shard_mask) };

        (hash, shard)
    }

    #[inline]
    fn get_hash_and_shard_for_histogram(
        &self,
        key: &K,
    ) -> (u64, &RwLock<RegistryHashMap<K, S::Histogram>>) {
        let hash = key.hashable();

        // SAFETY: We initialize the vector of subshards with a power-of-two value, and
        // `self.shard_mask` is `self.histograms.len() - 1`, thus we can never have a result from
        // the masking operation that results in a value which is not in bounds of our subshards
        // vector.
        let shard = unsafe { self.histograms.get_unchecked(hash as usize & self.shard_mask) };

        (hash, shard)
    }
}

impl<K, S> Registry<K, S>
where
    S: Storage<K>,
    K: Eq + Hashable,
{
    /// Deletes a counter from the registry.
    ///
    /// Returns `true` if the counter existed and was removed, `false` otherwise.
    pub fn delete_counter(&self, key: &K) -> bool {
        let (hash, shard) = self.get_hash_and_shard_for_counter(key);
        let mut shard_write = shard.write().unwrap_or_else(PoisonError::into_inner);
        let entry = shard_write.raw_entry_mut().from_key_hashed_nocheck(hash, key);
        if let RawEntryMut::Occupied(entry) = entry {
            let _ = entry.remove_entry();
            return true;
        }

        false
    }

    /// Deletes a gauge from the registry.
    ///
    /// Returns `true` if the gauge existed and was removed, `false` otherwise.
    pub fn delete_gauge(&self, key: &K) -> bool {
        let (hash, shard) = self.get_hash_and_shard_for_gauge(key);
        let mut shard_write = shard.write().unwrap_or_else(PoisonError::into_inner);
        let entry = shard_write.raw_entry_mut().from_key_hashed_nocheck(hash, key);
        if let RawEntryMut::Occupied(entry) = entry {
            let _ = entry.remove_entry();
            return true;
        }

        false
    }

    /// Deletes a histogram from the registry.
    ///
    /// Returns `true` if the histogram existed and was removed, `false` otherwise.
    pub fn delete_histogram(&self, key: &K) -> bool {
        let (hash, shard) = self.get_hash_and_shard_for_histogram(key);
        let mut shard_write = shard.write().unwrap_or_else(PoisonError::into_inner);
        let entry = shard_write.raw_entry_mut().from_key_hashed_nocheck(hash, key);
        if let RawEntryMut::Occupied(entry) = entry {
            let _ = entry.remove_entry();
            return true;
        }

        false
    }

    /// Gets a copy of an existing counter.
    pub fn get_counter(&self, key: &K) -> Option<S::Counter> {
        let (hash, shard) = self.get_hash_and_shard_for_counter(key);
        let shard_read = shard.read().unwrap_or_else(PoisonError::into_inner);
        shard_read.raw_entry().from_key_hashed_nocheck(hash, key).map(|(_, v)| v.clone())
    }

    /// Gets a copy of an existing gauge.
    pub fn get_gauge(&self, key: &K) -> Option<S::Gauge> {
        let (hash, shard) = self.get_hash_and_shard_for_gauge(key);
        let shard_read = shard.read().unwrap_or_else(PoisonError::into_inner);
        shard_read.raw_entry().from_key_hashed_nocheck(hash, key).map(|(_, v)| v.clone())
    }

    /// Gets a copy of an existing histogram.
    pub fn get_histogram(&self, key: &K) -> Option<S::Histogram> {
        let (hash, shard) = self.get_hash_and_shard_for_histogram(key);
        let shard_read = shard.read().unwrap_or_else(PoisonError::into_inner);
        shard_read.raw_entry().from_key_hashed_nocheck(hash, key).map(|(_, v)| v.clone())
    }
}

impl<K, S> Registry<K, S>
where
    S: Storage<K>,
    K: Clone + Eq + Hashable,
{
    /// Gets or creates the given counter.
    ///
    /// The `op` function will be called for the counter under the given `key`, with the counter
    /// first being created if it does not already exist.
    pub fn get_or_create_counter<O, V>(&self, key: &K, op: O) -> V
    where
        O: FnOnce(&S::Counter) -> V,
    {
        let (hash, shard) = self.get_hash_and_shard_for_counter(key);

        // Try and get the handle if it exists, running our operation if we succeed.
        let shard_read = shard.read().unwrap_or_else(PoisonError::into_inner);
        if let Some((_, v)) = shard_read.raw_entry().from_key_hashed_nocheck(hash, key) {
            op(v)
        } else {
            // Switch to write guard and insert the handle first.
            drop(shard_read);
            let mut shard_write = shard.write().unwrap_or_else(PoisonError::into_inner);
            let v = if let Some((_, v)) = shard_write.raw_entry().from_key_hashed_nocheck(hash, key)
            {
                v
            } else {
                let (_, v) = shard_write
                    .raw_entry_mut()
                    .from_key_hashed_nocheck(hash, key)
                    .or_insert_with(|| (key.clone(), self.storage.counter(key)));

                v
            };

            op(v)
        }
    }

    /// Gets or creates the given gauge.
    ///
    /// The `op` function will be called for the gauge under the given `key`, with the gauge
    /// first being created if it does not already exist.
    pub fn get_or_create_gauge<O, V>(&self, key: &K, op: O) -> V
    where
        O: FnOnce(&S::Gauge) -> V,
    {
        let (hash, shard) = self.get_hash_and_shard_for_gauge(key);

        // Try and get the handle if it exists, running our operation if we succeed.
        let shard_read = shard.read().unwrap_or_else(PoisonError::into_inner);
        if let Some((_, v)) = shard_read.raw_entry().from_key_hashed_nocheck(hash, key) {
            op(v)
        } else {
            // Switch to write guard and insert the handle first.
            drop(shard_read);
            let mut shard_write = shard.write().unwrap_or_else(PoisonError::into_inner);
            let v = if let Some((_, v)) = shard_write.raw_entry().from_key_hashed_nocheck(hash, key)
            {
                v
            } else {
                let (_, v) = shard_write
                    .raw_entry_mut()
                    .from_key_hashed_nocheck(hash, key)
                    .or_insert_with(|| (key.clone(), self.storage.gauge(key)));

                v
            };

            op(v)
        }
    }

    /// Gets or creates the given histogram.
    ///
    /// The `op` function will be called for the histogram under the given `key`, with the histogram
    /// first being created if it does not already exist.
    pub fn get_or_create_histogram<O, V>(&self, key: &K, op: O) -> V
    where
        O: FnOnce(&S::Histogram) -> V,
    {
        let (hash, shard) = self.get_hash_and_shard_for_histogram(key);

        // Try and get the handle if it exists, running our operation if we succeed.
        let shard_read = shard.read().unwrap_or_else(PoisonError::into_inner);
        if let Some((_, v)) = shard_read.raw_entry().from_key_hashed_nocheck(hash, key) {
            op(v)
        } else {
            // Switch to write guard and insert the handle first.
            drop(shard_read);
            let mut shard_write = shard.write().unwrap_or_else(PoisonError::into_inner);
            let v = if let Some((_, v)) = shard_write.raw_entry().from_key_hashed_nocheck(hash, key)
            {
                v
            } else {
                let (_, v) = shard_write
                    .raw_entry_mut()
                    .from_key_hashed_nocheck(hash, key)
                    .or_insert_with(|| (key.clone(), self.storage.histogram(key)));

                v
            };

            op(v)
        }
    }
    /// Gets a map of all present counters, mapped by key.
    ///
    /// This map is a point-in-time snapshot of the registry.
    pub fn get_counter_handles(&self) -> HashMap<K, S::Counter> {
        let mut counters = HashMap::new();
        self.visit_counters(|k, v| {
            counters.insert(k.clone(), v.clone());
        });
        counters
    }

    /// Gets a map of all present gauges, mapped by key.
    ///
    /// This map is a point-in-time snapshot of the registry.
    pub fn get_gauge_handles(&self) -> HashMap<K, S::Gauge> {
        let mut gauges = HashMap::new();
        self.visit_gauges(|k, v| {
            gauges.insert(k.clone(), v.clone());
        });
        gauges
    }

    /// Gets a map of all present histograms, mapped by key.
    ///
    /// This map is a point-in-time snapshot of the registry.
    pub fn get_histogram_handles(&self) -> HashMap<K, S::Histogram> {
        let mut histograms = HashMap::new();
        self.visit_histograms(|k, v| {
            histograms.insert(k.clone(), v.clone());
        });
        histograms
    }
}

#[cfg(test)]
mod tests {
    use metrics::{atomics::AtomicU64, CounterFn, Key};

    use super::Registry;
    use std::sync::{atomic::Ordering, Arc};

    #[test]
    fn test_registry() {
        let registry = Registry::atomic();
        let key = Key::from_name("foobar");

        let entries = registry.get_counter_handles();
        assert_eq!(entries.len(), 0);

        assert!(registry.get_counter(&key).is_none());

        registry.get_or_create_counter(&key, |c: &Arc<AtomicU64>| c.increment(1));

        let initial_entries = registry.get_counter_handles();
        assert_eq!(initial_entries.len(), 1);

        let initial_entry: (Key, Arc<AtomicU64>) =
            initial_entries.into_iter().next().expect("failed to get first entry");

        let (ikey, ivalue) = initial_entry;
        assert_eq!(ikey, key);
        assert_eq!(ivalue.load(Ordering::SeqCst), 1);

        registry.get_or_create_counter(&key, |c: &Arc<AtomicU64>| c.increment(1));

        let updated_entries = registry.get_counter_handles();
        assert_eq!(updated_entries.len(), 1);

        let updated_entry: (Key, Arc<AtomicU64>) =
            updated_entries.into_iter().next().expect("failed to get updated entry");

        let (ukey, uvalue) = updated_entry;
        assert_eq!(ukey, key);
        assert_eq!(uvalue.load(Ordering::SeqCst), 2);

        let value = registry.get_counter(&key).expect("failed to get entry");
        assert!(Arc::ptr_eq(&value, &uvalue));

        registry.get_or_create_counter(&Key::from_name("baz"), |_| ());
        assert_eq!(registry.get_counter_handles().len(), 2);

        let mut n = 0;
        registry.retain_counters(|k, _| {
            n += 1;
            k.name().starts_with("foo")
        });
        assert_eq!(n, 2);
        assert_eq!(registry.get_counter_handles().len(), 1);

        assert!(registry.delete_counter(&key));

        let entries = registry.get_counter_handles();
        assert_eq!(entries.len(), 0);
    }
}
