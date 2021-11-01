use std::marker::PhantomData;
use std::sync::Arc;
use std::{hash::BuildHasherDefault, iter::repeat};

use atomic_shim::AtomicU64;
use hashbrown::{hash_map::RawEntryMut, HashMap};
use metrics::{CounterFn, GaugeFn, HistogramFn, Key, KeyHasher};
use parking_lot::RwLock;

use crate::{AtomicBucket, Hashable};

type RegistryHasher = KeyHasher;
type RegistryHashMap<V> = HashMap<Key, V, BuildHasherDefault<RegistryHasher>>;

pub trait Primitives {
    type Counter: CounterFn + Clone;
    type Gauge: GaugeFn + Clone;
    type Histogram: HistogramFn + Clone;

    fn counter() -> Self::Counter;
    fn gauge() -> Self::Gauge;
    fn histogram() -> Self::Histogram;
}

/// Standard metric primitives that fit most use cases.
///
/// The primitives used provide shared atomic access utilizing atomic storage and shared access via `Arc`.
pub struct StandardPrimitives;

impl Primitives for StandardPrimitives {
    type Counter = Arc<AtomicU64>;
    type Gauge = Arc<AtomicU64>;
    type Histogram = Arc<AtomicBucket<f64>>;

    fn counter() -> Self::Counter {
        Arc::new(AtomicU64::new(0))
    }

    fn gauge() -> Self::Gauge {
        Arc::new(AtomicU64::new(0))
    }

    fn histogram() -> Self::Histogram {
        Arc::new(AtomicBucket::new())
    }
}

/// A high-performance metric registry.
///
/// `Registry` provides the ability to maintain a central listing of metrics mapped by a given key.
///
/// In many cases, `K` will be a composite key, where the fundamental `Key` type from `metrics` is
/// present, and differentiation is provided by storing the metric type alongside.
///
/// Metrics themselves are represented opaquely behind `H`.  In most cases, this would be a
/// thread-safe handle to the underlying metrics storage that the owner of the registry can use to
/// update the actual metric value(s) as needed.  `Handle`, from this crate, is a solid default
/// choice.
///
/// As well, handles have an associated generation counter which is incremented any time an entry is
/// operated on.  This generation is returned with the handle when querying the registry, and can be
/// used in order to delete a handle from the registry, allowing callers to prune old/stale handles
/// over time.
///
/// `Registry` is optimized for reads.  
pub struct Registry<P = StandardPrimitives>
where
    P: Primitives,
{
    counters: Vec<RwLock<RegistryHashMap<P::Counter>>>,
    gauges: Vec<RwLock<RegistryHashMap<P::Gauge>>>,
    histograms: Vec<RwLock<RegistryHashMap<P::Histogram>>>,
    shard_mask: usize,
    _primitives: PhantomData<P>,
}

impl<P> Registry<P>
where
    P: Primitives,
{
    /// Creates a new `Registry`.
    pub fn new() -> Self {
        let shard_count = std::cmp::max(1, num_cpus::get()).next_power_of_two();
        let shard_mask = shard_count - 1;
        let counters = repeat(())
            .take(shard_count)
            .map(|_| RwLock::new(RegistryHashMap::default()))
            .collect();
        let gauges = repeat(())
            .take(shard_count)
            .map(|_| RwLock::new(RegistryHashMap::default()))
            .collect();
        let histograms = repeat(())
            .take(shard_count)
            .map(|_| RwLock::new(RegistryHashMap::default()))
            .collect();

        Self {
            counters,
            gauges,
            histograms,
            shard_mask,
            _primitives: PhantomData,
        }
    }

    #[inline]
    fn get_hash_and_shard_for_counter(
        &self,
        key: &Key,
    ) -> (u64, &RwLock<RegistryHashMap<P::Counter>>) {
        let hash = key.hashable();

        // SAFETY: We initialize vector of subshards with a power-of-two value, and
        // `self.shard_mask` is `self.counters.len() - 1`, thus we can never have a result from the
        // masking operation that results in a value which is not in bounds of our subshards vector.
        let shard = unsafe { self.counters.get_unchecked(hash as usize & self.shard_mask) };

        (hash, shard)
    }

    #[inline]
    fn get_hash_and_shard_for_gauge(&self, key: &Key) -> (u64, &RwLock<RegistryHashMap<P::Gauge>>) {
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
        key: &Key,
    ) -> (u64, &RwLock<RegistryHashMap<P::Histogram>>) {
        let hash = key.hashable();

        // SAFETY: We initialize the vector of subshards with a power-of-two value, and
        // `self.shard_mask` is `self.histograms.len() - 1`, thus we can never have a result from
        // the masking operation that results in a value which is not in bounds of our subshards
        // vector.
        let shard = unsafe {
            self.histograms
                .get_unchecked(hash as usize & self.shard_mask)
        };

        (hash, shard)
    }

    /// Removes all metrics from the registry.
    ///
    /// This operation is eventually consistent: metrics will be removed piecemeal, and this method
    /// does not ensure that callers will see the registry as entirely empty at any given point.
    pub fn clear(&self) {
        for shard in &self.counters {
            shard.write().clear();
        }
        for shard in &self.gauges {
            shard.write().clear();
        }
        for shard in &self.histograms {
            shard.write().clear();
        }
    }

    /// Gets or creates the given counter.
    ///
    /// The `op` function will be called for the counter under the given `key`, with the counter
    /// first being created if it does not already exist.
    pub fn get_or_create_counter<O, V>(&self, key: &Key, op: O) -> V
    where
        O: FnOnce(&P::Counter) -> V,
    {
        let (hash, shard) = self.get_hash_and_shard_for_counter(key);

        // Try and get the handle if it exists, running our operation if we succeed.
        let shard_read = shard.read();
        if let Some((_, v)) = shard_read.raw_entry().from_key_hashed_nocheck(hash, key) {
            op(v)
        } else {
            // Switch to write guard and insert the handle first.
            drop(shard_read);
            let mut shard_write = shard.write();
            let v = if let Some((_, v)) = shard_write.raw_entry().from_key_hashed_nocheck(hash, key)
            {
                v
            } else {
                let (_, v) = shard_write
                    .raw_entry_mut()
                    .from_key_hashed_nocheck(hash, key)
                    .or_insert_with(|| (key.clone(), P::counter()));

                v
            };

            op(v)
        }
    }

    /// Gets or creates the given gauge.
    ///
    /// The `op` function will be called for the gauge under the given `key`, with the gauge
    /// first being created if it does not already exist.
    pub fn get_or_create_gauge<O, V>(&self, key: &Key, op: O) -> V
    where
        O: FnOnce(&P::Gauge) -> V,
    {
        let (hash, shard) = self.get_hash_and_shard_for_gauge(key);

        // Try and get the handle if it exists, running our operation if we succeed.
        let shard_read = shard.read();
        if let Some((_, v)) = shard_read.raw_entry().from_key_hashed_nocheck(hash, key) {
            op(v)
        } else {
            // Switch to write guard and insert the handle first.
            drop(shard_read);
            let mut shard_write = shard.write();
            let v = if let Some((_, v)) = shard_write.raw_entry().from_key_hashed_nocheck(hash, key)
            {
                v
            } else {
                let (_, v) = shard_write
                    .raw_entry_mut()
                    .from_key_hashed_nocheck(hash, key)
                    .or_insert_with(|| (key.clone(), P::gauge()));

                v
            };

            op(v)
        }
    }

    /// Gets or creates the given histogram.
    ///
    /// The `op` function will be called for the histogram under the given `key`, with the histogram
    /// first being created if it does not already exist.
    pub fn get_or_create_histogram<O, V>(&self, key: &Key, op: O) -> V
    where
        O: FnOnce(&P::Histogram) -> V,
    {
        let (hash, shard) = self.get_hash_and_shard_for_histogram(key);

        // Try and get the handle if it exists, running our operation if we succeed.
        let shard_read = shard.read();
        if let Some((_, v)) = shard_read.raw_entry().from_key_hashed_nocheck(hash, key) {
            op(v)
        } else {
            // Switch to write guard and insert the handle first.
            drop(shard_read);
            let mut shard_write = shard.write();
            let v = if let Some((_, v)) = shard_write.raw_entry().from_key_hashed_nocheck(hash, key)
            {
                v
            } else {
                let (_, v) = shard_write
                    .raw_entry_mut()
                    .from_key_hashed_nocheck(hash, key)
                    .or_insert_with(|| (key.clone(), P::histogram()));

                v
            };

            op(v)
        }
    }

    /// Deletes a counter from the registry.
    ///
    /// Returns `true` if the counter existed and was removed, `false` otherwise.
    pub fn delete_counter(&self, key: &Key) -> bool {
        let (hash, shard) = self.get_hash_and_shard_for_counter(key);
        let mut shard_write = shard.write();
        let entry = shard_write
            .raw_entry_mut()
            .from_key_hashed_nocheck(hash, key);
        if let RawEntryMut::Occupied(entry) = entry {
            let _ = entry.remove_entry();
            return true;
        }

        false
    }

    /// Deletes a gauge from the registry.
    ///
    /// Returns `true` if the gauge existed and was removed, `false` otherwise.
    pub fn delete_gauge(&self, key: &Key) -> bool {
        let (hash, shard) = self.get_hash_and_shard_for_gauge(key);
        let mut shard_write = shard.write();
        let entry = shard_write
            .raw_entry_mut()
            .from_key_hashed_nocheck(hash, key);
        if let RawEntryMut::Occupied(entry) = entry {
            let _ = entry.remove_entry();
            return true;
        }

        false
    }

    /// Deletes a histogram from the registry.
    ///
    /// Returns `true` if the histogram existed and was removed, `false` otherwise.
    pub fn delete_histogram(&self, key: &Key) -> bool {
        let (hash, shard) = self.get_hash_and_shard_for_histogram(key);
        let mut shard_write = shard.write();
        let entry = shard_write
            .raw_entry_mut()
            .from_key_hashed_nocheck(hash, key);
        if let RawEntryMut::Occupied(entry) = entry {
            let _ = entry.remove_entry();
            return true;
        }

        false
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
        F: FnMut(&Key, &P::Counter),
    {
        for subshard in self.counters.iter() {
            let shard_read = subshard.read();
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
        F: FnMut(&Key, &P::Gauge),
    {
        for subshard in self.gauges.iter() {
            let shard_read = subshard.read();
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
        F: FnMut(&Key, &P::Histogram),
    {
        for subshard in self.histograms.iter() {
            let shard_read = subshard.read();
            for (key, histogram) in shard_read.iter() {
                collect(key, histogram);
            }
        }
    }

    /// Gets a map of all present counters, mapped by key.
    ///
    /// This map is a point-in-time snapshot of the registry.
    pub fn get_counter_handles(&self) -> HashMap<Key, P::Counter> {
        let mut counters = HashMap::new();
        self.visit_counters(|k, v| {
            counters.insert(k.clone(), v.clone());
        });
        counters
    }

    /// Gets a map of all present gauges, mapped by key.
    ///
    /// This map is a point-in-time snapshot of the registry.
    pub fn get_gauge_handles(&self) -> HashMap<Key, P::Gauge> {
        let mut gauges = HashMap::new();
        self.visit_gauges(|k, v| {
            gauges.insert(k.clone(), v.clone());
        });
        gauges
    }

    /// Gets a map of all present histograms, mapped by key.
    ///
    /// This map is a point-in-time snapshot of the registry.
    pub fn get_histogram_handles(&self) -> HashMap<Key, P::Histogram> {
        let mut histograms = HashMap::new();
        self.visit_histograms(|k, v| {
            histograms.insert(k.clone(), v.clone());
        });
        histograms
    }
}

#[cfg(test)]
mod tests {
    use atomic_shim::AtomicU64;
    use metrics::{CounterFn, Key};

    //use super::Generational;
    use super::Registry;
    use crate::registry::StandardPrimitives;
    //use crate::registry::Tracked;
    use std::sync::{atomic::Ordering, Arc};

    #[test]
    fn test_registry() {
        let registry = Registry::<StandardPrimitives>::new();
        let key = Key::from_name("foobar");

        let entries = registry.get_counter_handles();
        assert_eq!(entries.len(), 0);

        registry.get_or_create_counter(&key, |c: &Arc<AtomicU64>| c.increment(1));

        let initial_entries = registry.get_counter_handles();
        assert_eq!(initial_entries.len(), 1);

        let initial_entry: (Key, Arc<AtomicU64>) = initial_entries
            .into_iter()
            .next()
            .expect("failed to get first entry");

        let (ikey, ivalue) = initial_entry;
        assert_eq!(ikey, key);
        assert_eq!(ivalue.load(Ordering::SeqCst), 1);

        registry.get_or_create_counter(&key, |c: &Arc<AtomicU64>| c.increment(1));

        let updated_entries = registry.get_counter_handles();
        assert_eq!(updated_entries.len(), 1);

        let updated_entry: (Key, Arc<AtomicU64>) = updated_entries
            .into_iter()
            .next()
            .expect("failed to get updated entry");

        let (ukey, uvalue) = updated_entry;
        assert_eq!(ukey, key);
        assert_eq!(uvalue.load(Ordering::SeqCst), 2);

        assert!(registry.delete_counter(&key));

        let entries = registry.get_counter_handles();
        assert_eq!(entries.len(), 0);
    }
}
