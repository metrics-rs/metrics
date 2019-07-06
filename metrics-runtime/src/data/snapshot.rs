use crate::common::ValueSnapshot;
use metrics_core::{Key, Recorder, Snapshot as MetricsSnapshot};

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

impl MetricsSnapshot for Snapshot {
    /// Records the snapshot to the given recorder.
    fn record<R: Recorder>(&self, recorder: &mut R) {
        for (key, snapshot) in &self.measurements {
            let key = key.clone();
            match snapshot {
                ValueSnapshot::Counter(value) => recorder.record_counter(key, *value),
                ValueSnapshot::Gauge(value) => recorder.record_gauge(key, *value),
                ValueSnapshot::Histogram(stream) => stream.decompress_with(|values| {
                    recorder.record_histogram(key.clone(), values);
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MetricsSnapshot, Recorder, Snapshot, ValueSnapshot};
    use metrics_core::Key;
    use metrics_util::StreamingIntegers;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MockRecorder {
        counter: HashMap<Key, u64>,
        gauge: HashMap<Key, i64>,
        histogram: HashMap<Key, Vec<u64>>,
    }

    impl MockRecorder {
        pub fn get_counter_value(&self, key: &Key) -> Option<&u64> {
            self.counter.get(key)
        }

        pub fn get_gauge_value(&self, key: &Key) -> Option<&i64> {
            self.gauge.get(key)
        }

        pub fn get_histogram_values(&self, key: &Key) -> Option<&Vec<u64>> {
            self.histogram.get(key)
        }
    }

    impl Recorder for MockRecorder {
        fn record_counter(&mut self, key: Key, value: u64) {
            let _ = self.counter.insert(key, value);
        }

        fn record_gauge(&mut self, key: Key, value: i64) {
            let _ = self.gauge.insert(key, value);
        }

        fn record_histogram(&mut self, key: Key, values: &[u64]) {
            let _ = self.histogram.insert(key, values.to_vec());
        }
    }

    #[test]
    fn test_snapshot_recorder() {
        let key = Key::from_name("ok");
        let mut measurements = Vec::new();
        measurements.push((key.clone(), ValueSnapshot::Counter(7)));
        measurements.push((key.clone(), ValueSnapshot::Gauge(42)));

        let hvalues = vec![10, 25, 42, 97];
        let mut stream = StreamingIntegers::new();
        stream.compress(&hvalues);
        measurements.push((key.clone(), ValueSnapshot::Histogram(stream)));

        let snapshot = Snapshot::new(measurements);

        let mut recorder = MockRecorder::default();
        snapshot.record(&mut recorder);

        assert_eq!(recorder.get_counter_value(&key), Some(&7));
        assert_eq!(recorder.get_gauge_value(&key), Some(&42));

        let hsum = recorder.get_histogram_values(&key).map(|x| x.iter().sum());
        assert_eq!(hsum, Some(174));
    }
}
