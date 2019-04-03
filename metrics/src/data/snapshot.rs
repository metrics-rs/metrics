use super::histogram::HistogramSnapshot;
use metrics_core::MetricsExporter;
use std::fmt::Display;

/// A typed metric measurement, used in snapshots.
///
/// This type provides a way to wrap the value of a metric, for use in a snapshot, while also
/// providing the overall type of the metric, so that downstream consumers who how to properly
/// format the data.
#[derive(Debug, PartialEq, Eq)]
pub enum TypedMeasurement {
    Counter(String, u64),
    Gauge(String, i64),
    TimingHistogram(String, HistogramSnapshot),
    ValueHistogram(String, HistogramSnapshot),
}

/// A point-in-time view of metric data.
#[derive(Default, Debug)]
pub struct Snapshot {
    measurements: Vec<TypedMeasurement>,
}

impl Snapshot {
    /// Stores a counter value for the given metric key.
    pub(crate) fn set_count<T>(&mut self, key: T, value: u64)
    where
        T: Display,
    {
        self.measurements
            .push(TypedMeasurement::Counter(key.to_string(), value));
    }

    /// Stores a gauge value for the given metric key.
    pub(crate) fn set_gauge<T>(&mut self, key: T, value: i64)
    where
        T: Display,
    {
        self.measurements
            .push(TypedMeasurement::Gauge(key.to_string(), value));
    }

    /// Sets timing percentiles for the given metric key.
    ///
    /// From the given `HdrHistogram`, all the specific `percentiles` will be extracted and stored.
    pub(crate) fn set_timing_histogram<T>(&mut self, key: T, h: HistogramSnapshot)
    where
        T: Display,
    {
        self.measurements
            .push(TypedMeasurement::TimingHistogram(key.to_string(), h));
    }

    /// Sets value percentiles for the given metric key.
    ///
    /// From the given `HdrHistogram`, all the specific `percentiles` will be extracted and stored.
    pub(crate) fn set_value_histogram<T>(&mut self, key: T, h: HistogramSnapshot)
    where
        T: Display,
    {
        self.measurements
            .push(TypedMeasurement::ValueHistogram(key.to_string(), h));
    }

    /// Exports this [`Snapshot`] to the provided [`MetricsExporter`].
    pub fn export<M: MetricsExporter>(&self, exporter: &mut M) {
        for measurement in &self.measurements {
            match measurement {
                TypedMeasurement::Counter(key, value) => exporter.export_counter(key, *value),
                TypedMeasurement::Gauge(key, value) => exporter.export_gauge(key, *value),
                TypedMeasurement::TimingHistogram(key, hs) => {
                    for value in hs.values() {
                        exporter.export_histogram(key, *value);
                    }
                }
                TypedMeasurement::ValueHistogram(key, hs) => {
                    for value in hs.values() {
                        exporter.export_histogram(key, *value);
                    }
                }
            }
        }
    }

    /// Converts this [`Snapshot`] to the underlying vector of measurements.
    pub fn into_measurements(self) -> Vec<TypedMeasurement> {
        self.measurements
    }
}

#[cfg(test)]
mod tests {
    use super::{HistogramSnapshot, MetricsExporter, Snapshot, TypedMeasurement};
    use std::collections::HashMap;

    #[derive(Default)]
    struct MockExporter {
        counter: HashMap<String, u64>,
        gauge: HashMap<String, i64>,
        histogram: HashMap<String, u64>,
    }

    impl MockExporter {
        pub fn get_counter_value(&self, key: &String) -> Option<&u64> {
            self.counter.get(key)
        }

        pub fn get_gauge_value(&self, key: &String) -> Option<&i64> {
            self.gauge.get(key)
        }

        pub fn get_histogram_value(&self, key: &String) -> Option<&u64> {
            self.histogram.get(key)
        }
    }

    impl MetricsExporter for MockExporter {
        fn export_counter<K: AsRef<str>>(&mut self, key: K, value: u64) {
            let entry = self.counter.entry(key.as_ref().to_owned()).or_insert(0);
            *entry += value;
        }

        fn export_gauge<K: AsRef<str>>(&mut self, key: K, value: i64) {
            let entry = self.gauge.entry(key.as_ref().to_owned()).or_insert(0);
            *entry += value;
        }

        fn export_histogram<K: AsRef<str>>(&mut self, key: K, value: u64) {
            let entry = self.histogram.entry(key.as_ref().to_owned()).or_insert(0);
            *entry += value;
        }
    }

    #[test]
    fn test_snapshot_simple_set_and_get() {
        let key = "ok".to_owned();
        let mut snapshot = Snapshot::default();
        snapshot.set_count(key.clone(), 1);
        snapshot.set_gauge(key.clone(), 42);

        let values = snapshot.into_measurements();

        assert_eq!(values[0], TypedMeasurement::Counter(key.clone(), 1));
        assert_eq!(values[1], TypedMeasurement::Gauge(key.clone(), 42));
    }

    #[test]
    fn test_snapshot_exporter() {
        let key = "ok".to_owned();
        let mut snapshot = Snapshot::default();
        snapshot.set_count(key.clone(), 7);
        snapshot.set_gauge(key.clone(), 42);

        let hvalues = vec![10, 25, 42, 97];
        let histogram = HistogramSnapshot::new(hvalues);
        snapshot.set_timing_histogram(key.clone(), histogram);

        let mut exporter = MockExporter::default();
        snapshot.export(&mut exporter);

        assert_eq!(exporter.get_counter_value(&key), Some(&7));
        assert_eq!(exporter.get_gauge_value(&key), Some(&42));
        assert_eq!(exporter.get_histogram_value(&key), Some(&174));
    }
}
