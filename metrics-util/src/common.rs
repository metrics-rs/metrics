use std::fmt::Debug;
use std::hash::{Hash, Hasher};

use metrics::{Key, KeyHasher};

/// A type that can hash itself.
///
/// In high-performance use cases, an object can pre-hash itself, or memoize its hash value, when it
/// is anticipated that an object will be hashed multiple times. Rather than the standard library
/// `Hash` trait, `Hashable` exposes an interface that forces objects to hash themselves entirely,
/// providing only the resulting 8-byte hash.
///
/// For all implementations of `Hashable`, you _must_ utilize `metrics::KeyHasher`.  Usage of
/// another hasher will lead to inconsistency in places where `Hashable` is used, specifically
/// `Registry`.  You can wrap items in `DefaultHashable` to ensure they utilize the correct hasher.
pub trait Hashable: Hash {
    /// Generate the hash of this object.
    fn hashable(&self) -> u64;
}

impl Hashable for Key {
    fn hashable(&self) -> u64 {
        self.get_hash()
    }
}

#[derive(Debug, Hash, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct DefaultHashable<H: Hash>(pub H);

impl<H: Hash> Hashable for DefaultHashable<H> {
    fn hashable(&self) -> u64 {
        let mut hasher = KeyHasher::default();
        self.hash(&mut hasher);
        hasher.finish()
    }
}
