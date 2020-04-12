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
#[derive(Copy, Clone, Default, PartialEq, Eq, Hash)]
pub struct Identifier(usize);

impl Identifier {
    /// Creates a zeroed-out identifier.
    pub const fn zeroed() -> Identifier {
        Identifier(0)
    }
}

impl From<usize> for Identifier {
    fn from(v: usize) -> Self {
        Identifier(v)
    }
}

impl Into<usize> for Identifier {
    fn into(self) -> usize {
        self.0
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
            inner: UnsafeCell::new(Identifier::zeroed()),
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
