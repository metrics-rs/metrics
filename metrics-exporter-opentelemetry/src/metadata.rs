use metrics::{KeyName, SharedString, Unit};
use metrics_util::MetricKind;
use std::collections::HashMap;
use std::sync::{Arc, PoisonError, RwLock};

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
#[derive(Default)]
pub struct MetricMetadata {
    inner: Arc<RwLock<MetricMetadataInner>>,
}

#[derive(Default)]
struct MetricMetadataInner {
    descriptions: HashMap<(KeyName, MetricKind), MetricDescription>,
    histogram_bounds: HashMap<KeyName, Vec<f64>>,
}

impl MetricMetadata {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn add_description(
        &self,
        key_name: KeyName,
        metric_kind: MetricKind,
        unit: Option<Unit>,
        description: SharedString,
    ) {
        let new_entry = MetricDescription { unit, description };
        let mut inner = self.inner.write().unwrap_or_else(PoisonError::into_inner);
        inner.descriptions.insert((key_name, metric_kind), new_entry);
    }
    
    pub fn get_description(
        &self,
        key_name: &KeyName,
        metric_kind: MetricKind,
    ) -> Option<MetricDescription> {
        let inner = self.inner.read().unwrap_or_else(PoisonError::into_inner);
        inner.descriptions.get(&(key_name.clone(), metric_kind)).cloned()
    }
    
    pub fn set_histogram_bounds(&self, key_name: KeyName, bounds: Vec<f64>) {
        let mut inner = self.inner.write().unwrap_or_else(PoisonError::into_inner);
        inner.histogram_bounds.insert(key_name, bounds);
    }
    
    pub fn get_histogram_bounds(&self, key_name: &KeyName) -> Option<Vec<f64>> {
        let inner = self.inner.read().unwrap_or_else(PoisonError::into_inner);
        inner.histogram_bounds.get(key_name).cloned()
    }
}

impl Clone for MetricMetadata {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}