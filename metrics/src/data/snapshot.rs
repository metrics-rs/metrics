use crate::common::ValueSnapshot;
use metrics_core::{Recorder, Key, Snapshot as MetricsSnapshot};
use std::borrow::Cow;

/// A point-in-time view of metric data.
#[derive(Default, Debug)]
pub struct Snapshot {
    measurements: Vec<(String, ValueSnapshot)>,
}

impl Snapshot {
    pub(crate) fn from(from: Vec<(String, ValueSnapshot)>) -> Self {
        Snapshot { measurements: from }
    }
}

impl MetricsSnapshot for Snapshot {
    /// Records the snapshot to the given recorder.
    fn record<R: Recorder>(&self, recorder: &mut R) {
        for (key, snapshot) in &self.measurements {
            // TODO: switch this to Key::Owned once type_alias_enum_variants lands
            // in 1.37.0 (#61682)
            let owned_key: Key = Cow::Owned(key.clone());
            match snapshot {
                ValueSnapshot::Counter(value) => recorder.record_counter(owned_key.clone(), *value),
                ValueSnapshot::Gauge(value) => recorder.record_gauge(owned_key.clone(), *value),
                ValueSnapshot::Histogram(stream) => stream.decompress_with(|values| {
                    recorder.record_histogram(owned_key.clone(), values);
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
        counter: HashMap<String, u64>,
        gauge: HashMap<String, i64>,
        histogram: HashMap<String, Vec<u64>>,
    }

    impl MockRecorder {
        pub fn get_counter_value(&self, key: &String) -> Option<&u64> {
            self.counter.get(key)
        }

        pub fn get_gauge_value(&self, key: &String) -> Option<&i64> {
            self.gauge.get(key)
        }

        pub fn get_histogram_values(&self, key: &String) -> Option<&Vec<u64>> {
            self.histogram.get(key)
        }
    }

    impl Recorder for MockRecorder {
        fn record_counter<K: Into<Key>>(&mut self, key: K, value: u64) {
            let _ = self.counter.insert(key.into().to_string(), value);
        }

        fn record_gauge<K: Into<Key>>(&mut self, key: K, value: i64) {
            let _ = self.gauge.insert(key.into().to_string(), value);
        }

        fn record_histogram<K: Into<Key>>(&mut self, key: K, values: &[u64]) {
            let _ = self
                .histogram
                .insert(key.into().to_string(), values.to_vec());
        }
    }

    #[test]
    fn test_snapshot_recorder() {
        let key = "ok".to_owned();
        let mut measurements = Vec::new();
        measurements.push((key.clone(), ValueSnapshot::Counter(7)));
        measurements.push((key.clone(), ValueSnapshot::Gauge(42)));

        let hvalues = vec![10, 25, 42, 97];
        let mut stream = StreamingIntegers::new();
        stream.compress(&hvalues);
        measurements.push((key.clone(), ValueSnapshot::Histogram(stream)));

        let snapshot: Snapshot = Snapshot::from(measurements);

        let mut recorder = MockRecorder::default();
        snapshot.record(&mut recorder);

        assert_eq!(recorder.get_counter_value(&key), Some(&7));
        assert_eq!(recorder.get_gauge_value(&key), Some(&42));

        let hsum = recorder.get_histogram_values(&key).map(|x| x.iter().sum());
        assert_eq!(hsum, Some(174));
    }
}
