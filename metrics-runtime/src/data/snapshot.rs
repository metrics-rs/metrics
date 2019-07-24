use crate::common::Measurement;
use metrics_core::Key;

/// A collection of point-in-time metric measurements.
#[derive(Default, Debug)]
pub struct Snapshot {
    measurements: Vec<(Key, Measurement)>,
}

impl Snapshot {
    pub(crate) fn new(measurements: Vec<(Key, Measurement)>) -> Self {
        Self { measurements }
    }

    /// Number of measurements in this snapshot.
    pub fn len(&self) -> usize {
        self.measurements.len()
    }

    /// Whether or not the snapshot is empty.
    pub fn is_empty(&self) -> bool {
        self.measurements.len() != 0
    }

    /// Converts a [`Snapshot`] into the internal measurements.
    pub fn into_measurements(self) -> Vec<(Key, Measurement)> {
        self.measurements
    }
}
