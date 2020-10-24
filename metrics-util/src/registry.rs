use core::hash::Hash;
use dashmap::DashMap;
use std::collections::HashMap;

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
/// `Registry` handles deduplicating metrics, and will return the `Identifier` for an existing
/// metric if a caller attempts to reregister it.
///
/// `Registry` is optimized for reads.
pub struct Registry<K, H> {
    map: DashMap<K, H>,
}

impl<K, H> Registry<K, H>
where
    K: Eq + Hash + Clone,
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
        let valref = self.map.entry(key).or_insert_with(init);
        op(valref.value())
    }
}

impl<K, H> Registry<K, H>
where
    K: Eq + Hash + Clone + 'static,
    H: Clone + 'static,
{
    /// Gets a map of all present handles, mapped by key.
    ///
    /// Handles must implement `Clone`.  This map is a point-in-time snapshot of the registry.
    pub fn get_handles(&self) -> HashMap<K, H> {
        self.map
            .iter()
            .map(|item| (item.key().clone(), item.value().clone()))
            .collect()
    }
}
