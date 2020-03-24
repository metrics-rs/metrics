use std::borrow::Cow;

/// An allocation-optimized string.
///
/// We specify `ScopedString` to attempt to get the best of both worlds: flexibility to provide a
/// static or dynamic (owned) string, while retaining the performance benefits of being able to
/// take ownership of owned strings and borrows of completely static strings.
pub type ScopedString = Cow<'static, str>;

/// Opaque identifier for a metric.
#[derive(Default)]
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

impl Into<usize> for &Identifier {
    fn into(self) -> usize {
        self.0
    }
}
