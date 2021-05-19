use core::sync::atomic::{AtomicUsize, Ordering};
use std::{
    fmt::{self, Debug},
    marker::PhantomData,
};
use std::{hash::BuildHasherDefault, iter::repeat};

use hashbrown::{hash_map::RawEntryMut, HashMap};
use metrics::KeyHasher;
use parking_lot::RwLock;

use crate::{Hashable, MetricKind};

type RegistryHasher = KeyHasher;
type RegistryHashMap<K, V> = HashMap<K, V, BuildHasherDefault<RegistryHasher>>;

/// Generation counter.
///
/// Used for denoting the generation of a given handle, which is used to provide compare-and-swap
/// deletion semantics i.e. if the generation used to request deletion for a handle is behind the
/// current generation of the handle, then the deletion will not proceed.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Generation(usize);

/// A trait that defines generational semantics for wrapper types.
///
/// Used to provide compile-time generation tracking where the choice is encoded in which type is
/// used (i.e. `Tracked` vs `NotTracked`), which necessitates a trait to mediate usage.
pub trait Generational<H>: From<H> {
    /// Increments the generation counter.
    fn increment_generation(&self);

    /// Gets the current generation counter.
    fn get_generation(&self) -> Generation;

    /// Gets a reference to the inner type.
    fn get_inner(&self) -> &H;

    /// Creates a default initialized wrapper around `inner`.
    fn initial(inner: H) -> Self {
        inner.into()
    }
}

/// A generational wrapper that does track the generation.
pub struct Tracked<H>(AtomicUsize, H);

impl<H> Tracked<H> {
    /// Creates a new `Tracked`.
    pub fn new(h: H) -> Tracked<H> {
        Tracked(AtomicUsize::new(0), h)
    }
}

impl<H> Generational<H> for Tracked<H> {
    /// Increments the generation counter.
    fn increment_generation(&self) {
        self.0.fetch_add(1, Ordering::Release);
    }

    /// Gets the current generation counter.
    fn get_generation(&self) -> Generation {
        Generation(self.0.load(Ordering::Acquire))
    }

    /// Gets a reference to the inner type.
    fn get_inner(&self) -> &H {
        &self.1
    }
}

impl<H> From<H> for Tracked<H> {
    fn from(inner: H) -> Self {
        Self::new(inner)
    }
}

impl<H: fmt::Debug> fmt::Debug for Tracked<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tracked")
            .field("gen", &self.0)
            .field("inner", &self.1)
            .finish()
    }
}

/// A generational wrapper that does not track the generation.
pub struct NotTracked<H>(H);

impl<H> NotTracked<H> {
    /// Creates a new `NotTracked`.
    pub fn new(h: H) -> NotTracked<H> {
        NotTracked(h)
    }
}

impl<H> Generational<H> for NotTracked<H> {
    /// Increments the generation counter.
    fn increment_generation(&self) {}

    /// Gets the current generation counter.
    fn get_generation(&self) -> Generation {
        Generation::default()
    }

    /// Gets a reference to the inner type.
    fn get_inner(&self) -> &H {
        &self.0
    }
}

impl<H> From<H> for NotTracked<H> {
    fn from(inner: H) -> Self {
        Self::new(inner)
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
pub struct Registry<K, H, G>
where
    K: Eq + Hashable + Clone + 'static,
    H: 'static,
{
    shards: Vec<Vec<RwLock<RegistryHashMap<K, G>>>>,
    mask: usize,
    _handle: PhantomData<H>,
}

impl<K, H, G> Registry<K, H, G>
where
    K: Eq + Hashable + Clone + 'static,
    H: 'static,
{
    /// Creates a new `Registry`.
    fn new() -> Self {
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

        Self {
            shards,
            mask,
            _handle: PhantomData,
        }
    }

    /// Creates a new `Registry` without generation semantics.
    pub fn untracked() -> Registry<K, H, NotTracked<H>> {
        Registry::<K, H, NotTracked<H>>::new()
    }

    /// Creates a new `Registry` with generation semantics.
    ///
    /// In some use cases, there is a requirement to understand what "generation" a metric is, for
    /// the purposes of understanding if a given metric has changed between two points in time.
    ///
    /// This registry wraps metrics with a generation counter, which is incremented every time a
    /// metric is operated on.  When queried for a list of metrics, they'll be provided with their
    /// current generation.
    pub fn tracked() -> Registry<K, H, Tracked<H>> {
        Registry::<K, H, Tracked<H>>::new()
    }
}

impl<K, H, G> Registry<K, H, G>
where
    K: Eq + Hashable + Clone + 'static,
    H: 'static,
    G: Generational<H>,
{
    #[inline]
    fn get_hash_and_shard(
        &self,
        kind: MetricKind,
        key: &K,
    ) -> (u64, &RwLock<RegistryHashMap<K, G>>) {
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

    /// Removes all metrics from the registry.
    ///
    /// This operation is eventually consistent: metrics will be removed piecemeal, and this method
    /// does not ensure that callers will see the registry as entirely empty at any given point.
    pub fn clear(&self) {
        for shard in &self.shards {
            for subshard in shard {
                subshard.write().clear();
            }
        }
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
                let (_, v) = shard_write
                    .raw_entry_mut()
                    .from_key_hashed_nocheck(hash, key)
                    .or_insert_with(|| (key.clone(), G::initial(init())));

                v
            };

            let result = op(v.get_inner());
            v.increment_generation();
            result
        }
    }

    /// Deletes a handle from the registry.
    ///
    /// Returns `true` if the handle existed and was removed, `false` otherwise.
    pub fn delete(&self, kind: MetricKind, key: &K) -> bool {
        let (hash, shard) = self.get_hash_and_shard(kind, key);
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

    /// Deletes a handle from the registry.
    ///
    /// The generation of a given key is passed along when querying the registry via
    /// [`get_handles`](Registry::get_handles).  If the generation given here does not match the
    /// current generation, then the handle will not be removed.
    ///
    /// Returns `true` if the handle existed and was removed, `false` otherwise.
    pub fn delete_with_gen(&self, kind: MetricKind, key: &K, generation: Generation) -> bool {
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

    /// Visits every handle stored in this registry.
    ///
    /// The given function will be passed the metric kind as well as the key and handle references,
    /// which are wrapped in `Generational`-implementing holders.
    ///
    /// This operation does not lock the entire registry, but proceeds directly through the
    /// "subshards" that are kept internally.  As a result, all subshards will be visited, but a
    /// metric that existed at the exact moment that `visit` was called may not actually be observed
    /// if it is deleted before that subshard is reached.  Likewise, a metric that is added after
    /// the call to `visit`, but before `visit` finishes, may also not be observed.
    pub fn visit<F>(&self, mut collect: F)
    where
        F: FnMut(MetricKind, (&K, &G)),
    {
        for (idx, subshards) in self.shards.iter().enumerate() {
            let kind = idx_to_kind(idx);

            for subshard in subshards {
                let shard_read = subshard.read();
                for item in shard_read.iter() {
                    collect(kind, item);
                }
            }
        }
    }

    /// Gets a map of all present handles, mapped by key.
    ///
    /// Handles must implement `Clone`.  This map is a point-in-time snapshot of the registry.
    pub fn get_handles(&self) -> HashMap<(MetricKind, K), (Generation, H)>
    where
        H: Clone,
    {
        let mut handles = HashMap::new();
        self.visit(|kind, (k, v)| {
            handles.insert(
                (kind, k.clone()),
                (v.get_generation(), v.get_inner().clone()),
            );
        });
        handles
    }
}

impl<K, H, G> Default for Registry<K, H, G>
where
    K: Eq + Hashable + Clone + 'static,
    H: 'static,
    G: Generational<H>,
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
    use crate::{DefaultHashable, NotTracked, Tracked};
    use std::sync::{
        atomic::{AtomicUsize, Ordering::SeqCst},
        Arc,
    };

    #[test]
    fn test_tracked() {
        let generational = Tracked::new(1);
        let start_gen = generational.get_generation();
        let start_gen_extra = generational.get_generation();
        assert_eq!(start_gen, start_gen_extra);

        generational.increment_generation();

        let end_gen = generational.get_generation();
        assert_ne!(start_gen, end_gen);
    }

    #[test]
    fn test_not_tracked() {
        let generational = NotTracked::new(1);
        let start_gen = generational.get_generation();
        let start_gen_extra = generational.get_generation();
        assert_eq!(start_gen, start_gen_extra);

        generational.increment_generation();

        let end_gen = generational.get_generation();
        assert_eq!(start_gen, end_gen);
    }

    #[test]
    fn test_tracked_registry() {
        let registry =
            Registry::<DefaultHashable<u64>, Arc<AtomicUsize>, Tracked<Arc<AtomicUsize>>>::default(
            );

        let key = DefaultHashable(1);

        let entries = registry.get_handles();
        assert_eq!(entries.len(), 0);

        let initial_value = registry.op(
            MetricKind::Counter,
            &key,
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

        let (ikey, (initial_gen, value)) = initial_entry;
        assert_eq!(ikey, (MetricKind::Counter, DefaultHashable(1)));
        assert_eq!(value.load(SeqCst), 43);

        let update_value = registry.op(
            MetricKind::Counter,
            &key,
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

        let ((kind, ukey), (updated_gen, value)) = updated_entry;
        assert_eq!(kind, MetricKind::Counter);
        assert_eq!(ukey, DefaultHashable(1));
        assert_eq!(value.load(SeqCst), 44);

        assert!(!registry.delete_with_gen(kind, &key, initial_gen));

        let entries = registry.get_handles();
        assert_eq!(entries.len(), 1);

        assert!(registry.delete_with_gen(kind, &key, updated_gen));

        let entries = registry.get_handles();
        assert_eq!(entries.len(), 0);
    }
}
