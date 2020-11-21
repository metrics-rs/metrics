use core::{
    hash::Hash,
    sync::atomic::{AtomicUsize, Ordering},
};
use dashmap::DashMap;
use std::collections::HashMap;

#[derive(Debug)]
struct Generational<H>(AtomicUsize, H);

impl<H: Clone> Generational<H> {
    pub fn new(h: H) -> Generational<H> {
        Generational(AtomicUsize::new(0), h)
    }

    pub fn increment_generation(&self) {
        self.0.fetch_add(1, Ordering::Release);
    }

    pub fn get_generation(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }

    pub fn get_inner(&self) -> &H {
        &self.1
    }

    pub fn to_owned(&self) -> (usize, H) {
        (self.get_generation(), self.get_inner().clone())
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
pub struct Registry<K, H> {
    map: DashMap<K, Generational<H>>,
}

impl<K, H> Registry<K, H>
where
    K: Eq + Hash + Clone + 'static,
    H: Clone + 'static,
{
    /// Creates a new `Registry`.
    pub fn new() -> Self {
        Self {
            map: DashMap::new(),
        }
    }

    /// Perform an operation on a given key.
    ///
    /// The `op` function will be called for the handle under the given `key`.
    ///
    /// If the `key` is not already mapped, the `init` function will be
    /// called, and the resulting handle will be stored in the registry.
    pub fn op<I, O, V>(&self, key: K, op: O, init: I) -> V
    where
        I: FnOnce() -> H,
        O: FnOnce(&H) -> V,
    {
        let valref = self.map.entry(key).or_insert_with(|| {
            let value = init();
            Generational::new(value)
        });
        let value = valref.value();
        let result = op(value.get_inner());
        value.increment_generation();
        result
    }

    /// Deletes a handle from the registry.
    ///
    /// The generation of a given key is passed along when querying the registry via
    /// [`get_handles`](Register::get_handles).  If the generation given here does not match the
    /// current generation, then the handle will not be removed.
    pub fn delete(&self, key: &K, generation: usize) -> bool {
        self.map
            .remove_if(key, |_, g| g.get_generation() == generation)
            .is_some()
    }

    /// Gets a map of all present handles, mapped by key.
    ///
    /// Handles must implement `Clone`.  This map is a point-in-time snapshot of the registry.
    pub fn get_handles(&self) -> HashMap<K, (usize, H)> {
        self.map
            .iter()
            .map(|item| (item.key().clone(), item.value().to_owned()))
            .collect()
    }
}
