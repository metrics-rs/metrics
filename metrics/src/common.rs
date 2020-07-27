use std::borrow::Cow;
use std::cell::UnsafeCell;
use std::sync::Once;

/// An allocation-optimized string.
///
/// We specify `ScopedString` to attempt to get the best of both worlds: flexibility to provide a
/// static or dynamic (owned) string, while retaining the performance benefits of being able to
/// take ownership of owned strings and borrows of completely static strings.
pub type ScopedString = Cow<'static, str>;

/// Opaque identifier for a metric.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Identifier {
    /// An uninitialized or invalid identifier.
    ///
    /// Used either as a default value where static construction is required, or in special cases
    /// where an invalid identifier must be returned to signal downstream layers to not process a
    /// particular call i.e. filtering metrics by returning an invalid identifier during registration.
    Invalid,

    /// A valid identifier.
    Valid(usize),
}

impl Default for Identifier {
    fn default() -> Self {
        Identifier::Invalid
    }
}

impl From<usize> for Identifier {
    fn from(v: usize) -> Self {
        Identifier::Valid(v)
    }
}

/// Atomically-guarded identifier initialization.
///
/// Stores an identifier in an atomically-backed fashion, allowing for multiple callers to
/// race on creating the identifier, as well as waiting until it has been created, before
/// being able to take a reference to it.
pub struct OnceIdentifier {
    init: Once,
    inner: UnsafeCell<Identifier>,
}

impl OnceIdentifier {
    /// Creates a new `OnceIdentifier` in the uninitialized state.
    pub const fn new() -> OnceIdentifier {
        OnceIdentifier {
            init: Once::new(),
            inner: UnsafeCell::new(Identifier::Invalid),
        }
    }

    /// Gets or initializes the identifier.
    ///
    /// If the identifier has not yet been initialized, `f` is run to acquire it, and
    /// stores the identifier for other callers to utilize.
    ///
    /// All callers rondezvous on an internal atomic guard, so it impossible to see
    /// invalid state.
    pub fn get_or_init<F>(&self, f: F) -> Identifier
    where
        F: Fn() -> Identifier,
    {
        self.init.call_once(|| {
            let id = f();
            unsafe {
                (*self.inner.get()) = id;
            }
        });

        unsafe { *self.inner.get() }
    }
}

unsafe impl Sync for OnceIdentifier {}

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
