use std::borrow::Cow;

/// An allocation-optimized string.
///
/// We specify `ScopedString` to attempt to get the best of both worlds: flexibility to provide a
/// static or dynamic (owned) string, while retaining the performance benefits of being able to
/// take ownership of owned strings and borrows of completely static strings.
pub type ScopedString = Cow<'static, str>;

/// An object which can be converted into a `u64` representation.
///
/// This trait provides a mechanism for existing types, which have a natural representation
/// as an unsigned 64-bit integer, to be transparently passed in when recording a histogram.
pub trait IntoU64 {
    /// Converts this object to its `u64` representation.
    fn into_u64(self) -> u64;
}

impl IntoU64 for u64 {
    fn into_u64(self) -> u64 {
        self
    }
}

impl IntoU64 for std::time::Duration {
    fn into_u64(self) -> u64 {
        self.as_nanos() as u64
    }
}

/// Helper method to allow monomorphization of values passed to the `histogram!` macro.
#[doc(hidden)]
pub fn __into_u64<V: IntoU64>(value: V) -> u64 {
    value.into_u64()
}
