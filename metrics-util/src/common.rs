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
    fn hashable(&self) -> u64 {
        let mut hasher = Self::Hasher::default();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

impl Hashable for Key {
    type Hasher = KeyHasher;

    fn hashable(&self) -> u64 {
        self.get_hash()
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
    type Hasher = KeyHasher;

    fn hashable(&self) -> u64 {
        let mut hasher = KeyHasher::default();
        self.hash(&mut hasher);
        hasher.finish()
    }
}
