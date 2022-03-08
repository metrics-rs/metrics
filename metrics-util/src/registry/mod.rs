//! High-performance metrics storage.

mod storage;
pub use storage::{AtomicStorage, Storage};

#[cfg(feature = "recency")]
mod recency;

#[cfg(feature = "recency")]
pub use recency::{Generation, GenerationalAtomicStorage, Recency, Generational};

mod registry;
pub use registry::Registry;
