//! High-performance metrics storage.

mod storage;
pub use storage::{Storage, AtomicStorage};

#[cfg(feature = "recency")]
mod recency;

#[cfg(feature = "recency")]
pub use recency::{Generation, GenerationalAtomicStorage, Recency};

mod registry;
pub use registry::Registry;
