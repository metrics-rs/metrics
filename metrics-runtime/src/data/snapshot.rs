use crate::common::ValueSnapshot;
use metrics_core::Key;

/// A point-in-time view of metric data.
#[derive(Default, Debug)]
pub struct Snapshot {
    measurements: Vec<(Key, ValueSnapshot)>,
}

impl Snapshot {
    pub(crate) fn new(measurements: Vec<(Key, ValueSnapshot)>) -> Self {
        Snapshot { measurements }
    }

    /// Number of measurements in this snapshot.
    pub fn len(&self) -> usize {
        self.measurements.len()
    }

    /// Whether or not the snapshot is empty.
    pub fn is_empty(&self) -> bool {
        self.measurements.len() != 0
    }
}
