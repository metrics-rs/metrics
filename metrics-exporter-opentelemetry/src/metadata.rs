use metrics::{KeyName, SharedString, Unit};
use metrics_util::MetricKind;
use scc::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct MetricDescription {
    unit: Option<Unit>,
    description: SharedString,
}

impl MetricDescription {
    pub fn unit(&self) -> Option<Unit> {
        self.unit
    }
    
    pub fn description(&self) -> SharedString {
        self.description.clone()
    }
}

/// Stores all metric metadata including descriptions and histogram bounds
#[derive(Clone, Default)]
pub struct MetricMetadata {
    descriptions: Arc<HashMap<(KeyName, MetricKind), MetricDescription>>,
    histogram_bounds: Arc<HashMap<KeyName, Vec<f64>>>,
}

impl MetricMetadata {
    pub fn new() -> Self {
        Self {
            descriptions: Arc::new(HashMap::new()),
            histogram_bounds: Arc::new(HashMap::new()),
        }
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
