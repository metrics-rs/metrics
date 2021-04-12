use core::sync::atomic::{AtomicUsize, Ordering};
use std::{hash::BuildHasherDefault, iter::repeat};

use hashbrown::{hash_map::RawEntryMut, HashMap};
use parking_lot::RwLock;
use t1ha::T1haHasher;

use crate::{Hashable, MetricKind};

type RegistryHasher = T1haHasher;
type RegistryHashMap<K, V> = HashMap<K, Generational<V>, BuildHasherDefault<RegistryHasher>>;

/// Generation counter.
///
/// Used for denoting the generation of a given handle, which is used to provide compare-and-swap
/// deletion semantics i.e. if the generation used to request deletion for a handle is behind the
/// current generation of the handle, then the deletion will not proceed.
#[derive(Debug, Clone, PartialEq)]
pub struct Generation(usize);

#[derive(Debug)]
pub(crate) struct Generational<H>(AtomicUsize, H);

impl<H> Generational<H> {
    pub fn new(h: H) -> Generational<H> {
        Generational(AtomicUsize::new(0), h)
    }

    pub fn increment_generation(&self) {
        self.0.fetch_add(1, Ordering::Release);
    }

    pub fn get_generation(&self) -> Generation {
        Generation(self.0.load(Ordering::Acquire))
    }

    pub fn get_inner(&self) -> &H {
        &self.1
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
#[derive(Debug)]
pub struct Registry<K, H>
where
    K: Eq + Hashable + Clone + 'static,
    H: 'static,
{
    shards: Vec<Vec<RwLock<RegistryHashMap<K, H>>>>,
    mask: usize,
}

impl<K, H> Registry<K, H>
where
    K: Eq + Hashable + Clone + 'static,
    H: 'static,
{
    /// Creates a new `Registry`.
    pub fn new() -> Self {
        let shard_count = std::cmp::max(1, num_cpus::get()).next_power_of_two();
        let mask = shard_count - 1;
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

        let shards = vec![counters, gauges, histograms];

        Self { shards, mask }
    }

    #[inline]
    fn get_hash_and_shard(
        &self,
        kind: MetricKind,
        key: &K,
    ) -> (u64, &RwLock<RegistryHashMap<K, H>>) {
        let hash = key.hashable();

        // SAFETY: We map each MetricKind variant -- three at present -- to a usize value
        // representing an index in a vector, so we statically know that we're always extracting our
        // sub-shards correctly.  Secondly, we initialize vector of subshards with a power-of-two
        // value, and `self.mask` is `self.shards.len() - 1`, thus we can never have a result from
        // the masking operation that results in a value which is not in bounds of our subshards
        // vector.
        let shards = unsafe { self.shards.get_unchecked(kind_to_idx(kind)) };
        let shard = unsafe { shards.get_unchecked(hash as usize & self.mask) };

        (hash, shard)
    }

    /// Perform an operation on a given key.
    ///
    /// The `op` function will be called for the handle under the given `key`.
    ///
    /// If the `key` is not already mapped, the `init` function will be
    /// called, and the resulting handle will be stored in the registry.
    pub fn op<I, O, V>(&self, kind: MetricKind, key: &K, op: O, init: I) -> V
    where
        I: FnOnce() -> H,
        O: FnOnce(&H) -> V,
    {
        let (hash, shard) = self.get_hash_and_shard(kind, key);

        // Try and get the handle if it exists, running our operation if we succeed.
        let shard_read = shard.read();
        if let Some((_, v)) = shard_read.raw_entry().from_key_hashed_nocheck(hash, key) {
            let result = op(v.get_inner());
            v.increment_generation();
            result
        } else {
            // Switch to write guard and insert the handle first.
            drop(shard_read);
            let mut shard_write = shard.write();
            let v = if let Some((_, v)) = shard_write.raw_entry().from_key_hashed_nocheck(hash, key)
            {
                v
            } else {
                shard_write.entry(key.clone()).or_insert_with(|| {
                    let value = init();
                    Generational::new(value)
                })
            };

            let result = op(v.get_inner());
            v.increment_generation();
            result
        }
    }

    /// Deletes a handle from the registry.
    ///
    /// The generation of a given key is passed along when querying the registry via
    /// [`get_handles`](Registry::get_handles).  If the generation given here does not match the
    /// current generation, then the handle will not be removed.
    pub fn delete(&self, kind: MetricKind, key: &K, generation: Generation) -> bool {
        let (hash, shard) = self.get_hash_and_shard(kind, key);
        let mut shard_write = shard.write();
        let entry = shard_write
            .raw_entry_mut()
            .from_key_hashed_nocheck(hash, key);
        if let RawEntryMut::Occupied(entry) = entry {
            if entry.get().get_generation() == generation {
                let _ = entry.remove_entry();
                return true;
            }
        }

        false
    }

    /// Gets a map of all present handles, mapped by key.
    ///
    /// Handles must implement `Clone`.  This map is a point-in-time snapshot of the registry.
    pub fn get_handles(&self) -> HashMap<(MetricKind, K), (Generation, H)>
    where
        H: Clone,
    {
        self.shards
            .iter()
            .enumerate()
            .fold(HashMap::default(), |mut acc, (idx, subshards)| {
                let kind = idx_to_kind(idx);

                for subshard in subshards {
                    let shard_read = subshard.read();
                    let items = shard_read.iter().map(|(k, v)| {
                        (
                            (kind, k.clone()),
                            (v.get_generation(), v.get_inner().clone()),
                        )
                    });
                    acc.extend(items);
                }
                acc
            })
    }
}

impl<K, H> Default for Registry<K, H>
where
    K: Eq + Hashable + Clone + 'static,
    H: 'static,
{
    fn default() -> Self {
        Registry::new()
    }
}

const fn kind_to_idx(kind: MetricKind) -> usize {
    match kind {
        MetricKind::Counter => 0,
        MetricKind::Gauge => 1,
        MetricKind::Histogram => 2,
    }
}

#[inline]
fn idx_to_kind(idx: usize) -> MetricKind {
    match idx {
        0 => MetricKind::Counter,
        1 => MetricKind::Gauge,
        2 => MetricKind::Histogram,
        _ => panic!("invalid index"),
    }
}

#[cfg(test)]
mod tests {
    use super::{Generational, MetricKind, Registry};
    use std::sync::{
        atomic::{AtomicUsize, Ordering::SeqCst},
        Arc,
    };

    #[test]
    fn test_generation() {
        let generational = Generational::new(());
        let start_gen = generational.get_generation();
        let start_gen_extra = generational.get_generation();
        assert_eq!(start_gen, start_gen_extra);

        generational.increment_generation();

        let end_gen = generational.get_generation();
        assert_ne!(start_gen, end_gen);
    }

    #[test]
    fn test_registry() {
        let registry = Registry::<u64, Arc<AtomicUsize>>::new();

        let entries = registry.get_handles();
        assert_eq!(entries.len(), 0);

        let initial_value = registry.op(
            MetricKind::Counter,
            &1,
            |h| h.fetch_add(1, SeqCst),
            || Arc::new(AtomicUsize::new(42)),
        );
        assert_eq!(initial_value, 42);

        let initial_entries = registry.get_handles();
        assert_eq!(initial_entries.len(), 1);

        let initial_entry = initial_entries
            .into_iter()
            .next()
            .expect("failed to get first entry");

        let (key, (initial_gen, value)) = initial_entry;
        assert_eq!(key, (MetricKind::Counter, 1));
        assert_eq!(value.load(SeqCst), 43);

        let update_value = registry.op(
            MetricKind::Counter,
            &1,
            |h| h.fetch_add(1, SeqCst),
            || Arc::new(AtomicUsize::new(42)),
        );
        assert_eq!(update_value, 43);

        let updated_entries = registry.get_handles();
        assert_eq!(updated_entries.len(), 1);

        let updated_entry = updated_entries
            .into_iter()
            .next()
            .expect("failed to get updated entry");

        let ((kind, key), (updated_gen, value)) = updated_entry;
        assert_eq!(kind, MetricKind::Counter);
        assert_eq!(key, 1);
        assert_eq!(value.load(SeqCst), 44);

        assert!(!registry.delete(kind, &key, initial_gen));

        let entries = registry.get_handles();
        assert_eq!(entries.len(), 1);

        assert!(registry.delete(kind, &key, updated_gen));

        let entries = registry.get_handles();
        assert_eq!(entries.len(), 0);
    }
}
