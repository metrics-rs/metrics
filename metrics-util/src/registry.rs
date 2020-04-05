use std::sync::Arc;

use arc_swap::ArcSwap;
use im::HashMap;
use metrics::{Identifier, Key};
use parking_lot::Mutex;
use sharded_slab::Slab;

pub use sharded_slab::Guard;

/// A high-performance metric registry.
///
/// All metrics are defined by a `Key`, which represents the name of a metric, along with potential
/// labels.  Registering a new metric, in turn, provides the caller with an opaque `Identifier`
/// that can be used to look up the associated handle with a metric.
///
/// Handles would usually be a thread-safe type that can be used to manipulate a metric -- i.e.
/// increment a counter or add a value to a histogram -- but `Registry` does not care what data is
/// stored, it focuses purely on providing fast insertion and lookup.
///
/// `Registry` is optimized for reads.
pub struct Registry<H> {
    mappings: ArcSwap<HashMap<Key, Identifier>>,
    handles: Slab<H>,
    lock: Mutex<()>,
}

impl<H> Registry<H> {
    /// Creates a new `Registry`.
    pub fn new() -> Self {
        Registry {
            mappings: ArcSwap::from(Arc::new(HashMap::new())),
            handles: Slab::new(),
            lock: Mutex::new(()),
        }
    }

    /// Get or create a new identifier for a given key.
    ///
    /// If the key is not already mapped, a new identifier will be generated, and the given handle
    /// stored along side of it.  If the key is already mapped, its identifier will be returned.
    pub fn get_or_create_identifier(&self, key: Key, handle: H) -> Identifier {
        // Check our mapping table first.
        if let Some(id) = self.mappings.load().get(&key) {
            return id.clone();
        }

        // Take control of the registry.
        let guard = self.lock.lock();

        // Check our mapping table again, in case someone just inserted what we need.
        let mappings = self.mappings.load();
        if let Some(id) = mappings.get(&key) {
            return id.clone();
        }

        // Our identifier will be the index we insert the handle into.
        let id = self
            .handles
            .insert(handle)
            .expect("current thread ran out of slots to register new metrics!")
            .into();

        // Update our mapping table and drop the lock.
        let new_mappings = mappings.update(key, id);
        drop(mappings);
        self.mappings.store(Arc::new(new_mappings));
        drop(guard);

        id
    }

    /// Gets the handle for a given identifier.
    pub fn get_handle(&self, identifier: &Identifier) -> Option<Guard<'_, H>> {
        self.handles.get(identifier.into())
    }
}
