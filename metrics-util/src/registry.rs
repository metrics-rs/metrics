use core::{
    hash::Hash,
    sync::atomic::{AtomicUsize, Ordering},
};
use dashmap::DashMap;
use std::collections::HashMap;

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
    K: Eq + Hash + Clone + 'static,
    H: 'static,
{
    map: DashMap<K, Generational<H>>,
}

impl<K, H> Registry<K, H>
where
    K: Eq + Hash + Clone + 'static,
    H: 'static,
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
    /// [`get_handles`](Registry::get_handles).  If the generation given here does not match the
    /// current generation, then the handle will not be removed.
    pub fn delete(&self, key: &K, generation: Generation) -> bool {
        self.map
            .remove_if(key, |_, g| g.get_generation() == generation)
            .is_some()
    }

    /// Gets a map of all present handles, mapped by key.
    ///
    /// Handles must implement `Clone`.  This map is a point-in-time snapshot of the registry.
    pub fn get_handles(&self) -> HashMap<K, (Generation, H)>
    where
        H: Clone,
    {
        self.collect()
    }

    /// Collects all present key and associated generation/handle pairs
    /// into the provided type `T`.
    ///
    /// Handles must implement `Clone`.
    /// This collected result is a point-in-time snapshot of the registry.
    pub fn collect<T>(&self) -> T
    where
        H: Clone,
        T: std::iter::FromIterator<(K, (Generation, H))>,
    {
        self.map_collect(|key, generation, handle| (key.clone(), (generation, handle.clone())))
    }

    /// Maps and then collects all present key and associated generation/handle
    /// pairs into the provided type `T`.
    ///
    /// This map is appied over the values from a point-in-time snapshot of
    /// the registry.
    pub fn map_collect<F, R, T>(&self, mut f: F) -> T
    where
        F: for<'a> FnMut(&'a K, Generation, &'a H) -> R,
        T: std::iter::FromIterator<R>,
    {
        self.map
            .iter()
            .map(|item| {
                let value = item.value();
                f(item.key(), value.get_generation(), value.get_inner())
            })
            .collect()
    }
}

impl<K, H> Default for Registry<K, H>
where
    K: Eq + Hash + Clone + 'static,
    H: 'static,
{
    fn default() -> Self {
        Registry::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{Generational, Registry};
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
        let registry = Registry::<i32, Arc<AtomicUsize>>::new();

        let entries = registry.get_handles();
        assert_eq!(entries.len(), 0);

        let initial_value = registry.op(
            1,
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
        assert_eq!(key, 1);
        assert_eq!(value.load(SeqCst), 43);

        let update_value = registry.op(
            1,
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

        let (key, (updated_gen, value)) = updated_entry;
        assert_eq!(key, 1);
        assert_eq!(value.load(SeqCst), 44);

        assert!(!registry.delete(&key, initial_gen));

        let entries = registry.get_handles();
        assert_eq!(entries.len(), 1);

        assert!(registry.delete(&key, updated_gen));

        let entries = registry.get_handles();
        assert_eq!(entries.len(), 0);
    }
}
