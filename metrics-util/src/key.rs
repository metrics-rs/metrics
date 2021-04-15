use crate::MetricKind;
use metrics::Key;

/// A composite key that stores both the metric key and the metric kind.
///
/// This type is intended to be used by recorders that internally take advantage of
/// the provided [`Registry`](crate::registry::Registry) type when also using
/// [`Handle`](crate::handle::Handle).
///
/// Since handles are opaque, having the standard key by itself could lead to two
/// different metric kinds tryin to use the same key, calling read or write methods
/// that inevitably panic.  With `CompositeKey`, the kind can tied to the underlying
/// handle, ensuring parity between the two.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct CompositeKey(MetricKind, Key);

impl CompositeKey {
    /// Creates a new `CompositeKey`.
    pub const fn new(kind: MetricKind, key: Key) -> CompositeKey {
        CompositeKey(kind, key)
    }

    /// Gets the inner key represented by this `CompositeKey`.
    pub fn key(&self) -> &Key {
        &self.1
    }

    /// Gets the inner kind represented by this `CompositeKey`.
    pub fn kind(&self) -> MetricKind {
        self.0
    }

    /// Takes the individual pieces of this `CompositeKey`.
    pub fn into_parts(self) -> (MetricKind, Key) {
        (self.0, self.1)
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use metrics::Key;

    use super::*;

    #[test]
    fn test_same_keys_different_kinds_not_equal() {
        let key = Key::from_name("test");
        let key1 = CompositeKey::new(MetricKind::Counter, key.clone());
        let key2 = CompositeKey::new(MetricKind::Gauge, key);

        assert_ne!(key1.cmp(&key2), Ordering::Equal);
    }
}
