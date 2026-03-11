use std::fmt::Debug;
use std::hash::{Hash, Hasher};

use metrics::Key;
use rapidhash::fast::RapidHasher;

/// A type that can hash itself.
///
/// In high-performance use cases, an object can pre-hash itself, or memoize its hash value, when it
/// is anticipated that an object will be hashed multiple times. Rather than the standard library
/// `Hash` trait, `Hashable` exposes an interface that forces objects to hash themselves entirely,
/// providing only the resulting 8-byte hash.
///
/// As a key may sometimes need to be rehashed, we need to ensure that the same hashing algorithm
/// used to pre-generate the hash for this value is used when rehashing it.  All implementors must
/// define the hashing algorithm used by specifying the `Hasher` associated type.
///
/// A default implementation, [`DefaultHashable`], is provided that utilizes the same hashing
/// algorithm that [`Key`][metrics::Key] uses, which is high-performance.  This type can be used to
/// satisfy `Hashable` so long as the type itself is already [`Hash`].
pub trait Hashable: Hash {
    /// The hasher implementation used internally.
    type Hasher: Hasher + Default;

    /// Generate the hash of this object.
    #[inline]
    fn hashable(&self) -> u64 {
        let mut hasher = Self::Hasher::default();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

impl Hashable for Key {
    type Hasher = KeyHasher;

    #[inline]
    fn hashable(&self) -> u64 {
        self.get_hash()
    }
}

/// A no-op hasher for pre-hashed [`Key`][metrics::Key] types.
///
/// This hasher is designed for use with [`Key`][metrics::Key], which pre-computes its hash at
/// construction time. When `Key::hash()` is called, it writes the pre-computed hash via
/// `write_u64()`, and `finish()` simply returns that value.
///
/// This ensures that `HashMap<Key, V, BuildHasherDefault<KeyHasher>>` lookups work correctly
/// when using raw_entry APIs with pre-computed hashes.
///
/// # Panics
///
/// Panics if `finish()` is called without first calling `write_u64()`, or if any write method
/// other than `write_u64()` is called. This hasher is specifically for pre-hashed keys only.
#[derive(Debug, Default)]
pub struct KeyHasher {
    hash: Option<u64>,
}

impl Hasher for KeyHasher {
    #[inline(always)]
    fn finish(&self) -> u64 {
        self.hash.expect("KeyHasher::finish() called without write_u64(); KeyHasher is only for pre-hashed Key types")
    }

    fn write(&mut self, _bytes: &[u8]) {
        panic!("KeyHasher::write() called; KeyHasher only supports write_u64() for pre-hashed Key types");
    }

    #[inline(always)]
    fn write_u64(&mut self, i: u64) {
        self.hash = Some(i);
    }
}

/// A wrapper type that provides `Hashable` for any type that is `Hash`.
///
/// As part of using [`Registry`][crate::registry::Registry], the chosen key type must implement
/// [`Hashable`].  For use cases where performance is not the utmost concern and there is no desire
/// to deal with pre-hashing keys, `DefaultHashable` can be used to wrap the key type and provide
/// the implementation of `Hashable` so long as `H` itself is `Hash`.
#[derive(Debug, Hash, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DefaultHashable<H: Hash>(pub H);

impl<H: Hash> Hashable for DefaultHashable<H> {
    type Hasher = RapidHasher<'static>;

    fn hashable(&self) -> u64 {
        let mut hasher = RapidHasher::default();
        self.hash(&mut hasher);
        hasher.finish()
    }
}
