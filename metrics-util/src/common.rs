use std::hash::Hash;

use metrics::Key;

/// A type that can hash itself.
///
/// In high-performance use cases, an object can pre-hash itself, or memoize its hash value, when it
/// is anticipated that an object will be hashed multiple times. Rather than the standard library
/// `Hash` trait, `Hashable` exposes an interface that forces objects to hash themselves entirely,
/// providing only the resulting 8-byte hash.
pub trait Hashable: Hash {
    /// Generate the hash of this object.
    fn hashable(&self) -> u64;
}

impl Hashable for Key {
    fn hashable(&self) -> u64 {
        self.get_hash()
    }
}

impl Hashable for u64 {
    fn hashable(&self) -> u64 {
        *self
    }
}
