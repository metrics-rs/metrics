//! An OpenTelemetry metrics exporter for `metrics`.
mod instruments;
mod storage;

use metrics::{Counter, Gauge, Histogram, Key, KeyName, Recorder, SharedString, Unit};
use metrics_util::registry::{Registry, Storage};
use opentelemetry::metrics::Meter;
use crate::storage::OtelMetricStorage;

/// The OpenTelemetry recorder.
pub struct OpenTelemetryRecorder {
    registry: Registry<Key, OtelMetricStorage>,
}

impl OpenTelemetryRecorder {
    /// Creates a new OpenTelemetry recorder with the given meter.
    pub fn new(meter: Meter) -> Self {
        let storage = OtelMetricStorage::new(meter);
        Self {
            registry: Registry::new(storage),
        }
    }
}

impl Recorder for OpenTelemetryRecorder {
    fn describe_counter(&self, _key_name: KeyName, _unit: Option<Unit>, _description: SharedString) {
        // Descriptions are handled when creating instruments
    }

    fn describe_gauge(&self, _key_name: KeyName, _unit: Option<Unit>, _description: SharedString) {
        // Descriptions are handled when creating instruments
    }

    fn describe_histogram(
        &self,
        _key_name: KeyName,
        _unit: Option<Unit>,
        _description: SharedString,
    ) {
        // Descriptions are handled when creating instruments
    }

    fn register_counter(&self, key: &Key, _metadata: &metrics::Metadata<'_>) -> Counter {
        self.registry.get_or_create_counter(key, |c| {
            Counter::from_arc(c.clone())
        })
    }

    fn register_gauge(&self, key: &Key, _metadata: &metrics::Metadata<'_>) -> Gauge {
        self.registry.get_or_create_gauge(key, |g| {
            Gauge::from_arc(g.clone())
        })
    }

    fn register_histogram(&self, key: &Key, _metadata: &metrics::Metadata<'_>) -> Histogram {
        self.registry.get_or_create_histogram(key, |h| {
            Histogram::from_arc(h.clone())
        })
    }
}