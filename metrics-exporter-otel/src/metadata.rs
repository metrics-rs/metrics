use metrics::{KeyName, SharedString, Unit};
use metrics_util::MetricKind;
use scc::HashMap;
use std::sync::Arc;

/// A metric description containing unit and textual description.
///
/// This structure holds the metadata associated with a metric, including its unit of
/// measurement and human-readable description. This information is used to enrich
/// the OpenTelemetry metric output.
#[derive(Clone)]
pub struct MetricDescription {
    /// The unit of measurement for this metric (e.g., bytes, seconds, count)
    unit: Option<Unit>,
    /// Human-readable description of what this metric measures
    description: SharedString,
}

impl MetricDescription {
    /// Returns the unit of measurement for this metric.
    pub fn unit(&self) -> Option<Unit> {
        self.unit
    }

    /// Returns the human-readable description of this metric.
    pub fn description(&self) -> SharedString {
        self.description.clone()
    }
}

/// Stores all metric metadata including descriptions and histogram bounds.
///
/// This structure maintains a centralized store of metadata for all metrics, providing
/// lock-free concurrent access through SCC (Scalable Concurrent Collections) HashMaps.
/// It stores both metric descriptions (with units) and custom histogram bucket boundaries.
///
/// # Thread Safety
///
/// This structure is designed for high-performance concurrent access. Multiple threads
/// can safely read and write metadata simultaneously with minimal contention.
#[derive(Clone, Default)]
pub struct MetricMetadata {
    descriptions: Arc<HashMap<(KeyName, MetricKind), MetricDescription>>,
    histogram_bounds: Arc<HashMap<KeyName, Vec<f64>>>,
}

impl MetricMetadata {
    pub fn new() -> Self {
        Self { descriptions: Arc::new(HashMap::new()), histogram_bounds: Arc::new(HashMap::new()) }
    }

    pub fn set_description(
        &self,
        key_name: KeyName,
        metric_kind: MetricKind,
        unit: Option<Unit>,
        description: SharedString,
    ) {
        let new_entry = MetricDescription { unit, description };
        let _ = self.descriptions.insert((key_name, metric_kind), new_entry);
    }

    pub fn get_description(
        &self,
        key_name: &KeyName,
        metric_kind: MetricKind,
    ) -> Option<MetricDescription> {
        self.descriptions.read(&(key_name.clone(), metric_kind), |_, v| v.clone())
    }

    pub fn set_histogram_bounds(&self, key_name: KeyName, bounds: Vec<f64>) {
        let _ = self.histogram_bounds.insert(key_name, bounds);
    }

    pub fn get_histogram_bounds(&self, key_name: &KeyName) -> Option<Vec<f64>> {
        self.histogram_bounds.read(key_name, |_, v| v.clone())
    }
}
