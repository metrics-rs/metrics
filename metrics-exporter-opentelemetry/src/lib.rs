//! An OpenTelemetry metrics exporter for `metrics`.
mod description;
mod instruments;
mod storage;

use crate::description::DescriptionTable;
use crate::storage::OtelMetricStorage;
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Recorder, SharedString, Unit};
use metrics_util::registry::Registry;
use metrics_util::MetricKind;
use opentelemetry::metrics::Meter;
use std::sync::Arc;

/// The OpenTelemetry recorder.
pub struct OpenTelemetryRecorder {
    registry: Registry<Key, OtelMetricStorage>,
    description_table: Arc<DescriptionTable>,
}

impl OpenTelemetryRecorder {
    /// Creates a new OpenTelemetry recorder with the given meter.
    pub fn new(meter: Meter) -> Self {
        let description_table = Arc::new(DescriptionTable::default());
        let storage = OtelMetricStorage::new(meter, description_table.clone());
        Self { registry: Registry::new(storage), description_table }
    }

    /// Gets a description entry for testing purposes.
    #[cfg(test)]
    pub fn get_description(
        &self,
        key_name: KeyName,
        metric_kind: MetricKind,
    ) -> Option<crate::description::DescriptionEntry> {
        self.description_table.get_describe(key_name, metric_kind)
    }
}

impl Recorder for OpenTelemetryRecorder {
    fn describe_counter(
        &self,
        _key_name: KeyName,
        _unit: Option<Unit>,
        _description: SharedString,
    ) {
        self.description_table.add_describe(_key_name, MetricKind::Counter, _unit, _description);
    }

    fn describe_gauge(&self, _key_name: KeyName, _unit: Option<Unit>, _description: SharedString) {
        self.description_table.add_describe(_key_name, MetricKind::Gauge, _unit, _description);
    }

    fn describe_histogram(
        &self,
        _key_name: KeyName,
        _unit: Option<Unit>,
        _description: SharedString,
    ) {
        self.description_table.add_describe(_key_name, MetricKind::Histogram, _unit, _description);
    }

    fn register_counter(&self, key: &Key, _metadata: &metrics::Metadata<'_>) -> Counter {
        self.registry.get_or_create_counter(key, |c| Counter::from_arc(c.clone()))
    }

    fn register_gauge(&self, key: &Key, _metadata: &metrics::Metadata<'_>) -> Gauge {
        self.registry.get_or_create_gauge(key, |g| Gauge::from_arc(g.clone()))
    }

    fn register_histogram(&self, key: &Key, _metadata: &metrics::Metadata<'_>) -> Histogram {
        self.registry.get_or_create_histogram(key, |h| Histogram::from_arc(h.clone()))
    }
}
