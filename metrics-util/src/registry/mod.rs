//! High-performance metrics storage.

mod storage;
/// Advanced registry configuration for controlling how keys are prepared before insertion.
pub mod storage_strategy;

use metrics::Key;
pub use storage::{AtomicStorage, Storage};
pub use storage_strategy::{CloneOnInsert, RetainOnInsert, StorageStrategy};

#[cfg(feature = "recency")]
mod recency;

#[cfg(feature = "recency")]
#[cfg_attr(docsrs, doc(cfg(feature = "recency")))]
pub use recency::{
    Generation, Generational, GenerationalAtomicStorage, GenerationalStorage, Recency,
};

/// A high-performance metric registry using clone-on-insert semantics.
///
/// For advanced control over key preparation, use [`storage_strategy::Registry`].
pub type Registry<K, S> = storage_strategy::Registry<K, S, CloneOnInsert>;

/// A high-performance metric registry that retains [`Key`] values before insertion.
pub type RetainedKeyRegistry<S> = storage_strategy::Registry<Key, S, RetainOnInsert>;

#[cfg(test)]
mod tests {
    use metrics::{atomics::AtomicU64, CounterFn, Key};
    use std::sync::{atomic::Ordering, Arc};

    use super::{storage_strategy, AtomicStorage, Registry, RetainedKeyRegistry, StorageStrategy};

    fn assert_registry_behavior<Strategy>(
        registry: storage_strategy::Registry<Key, AtomicStorage, Strategy>,
    ) where
        Strategy: StorageStrategy<Key>,
    {
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

    #[test]
    fn test_registry() {
        assert_registry_behavior(Registry::atomic());
    }

    #[test]
    fn test_retained_key_registry() {
        assert_registry_behavior(RetainedKeyRegistry::atomic());
    }
}
