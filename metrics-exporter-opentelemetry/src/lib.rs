#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg), deny(rustdoc::broken_intra_doc_links))]
#![deny(missing_docs)]

mod instruments;
mod metadata;
mod storage;

use crate::metadata::MetricMetadata;
use crate::storage::OtelMetricStorage;
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Recorder, SharedString, Unit};
use metrics_util::registry::Registry;
use metrics_util::MetricKind;
use opentelemetry::metrics::Meter;

/// A [`Recorder`] that exports metrics to OpenTelemetry.
///
/// ```rust,no_run
/// use opentelemetry::metrics::MeterProvider;
/// use metrics_exporter_opentelemetry::OpenTelemetryRecorder;
/// use opentelemetry_sdk::metrics::SdkMeterProvider;
///
/// let provider = SdkMeterProvider::default();
/// let meter = provider.meter("my_app");
/// let recorder = OpenTelemetryRecorder::new(meter);
///
/// metrics::set_global_recorder(recorder).expect("failed to install recorder");
/// ```
pub struct OpenTelemetryRecorder {
    registry: Registry<Key, OtelMetricStorage>,
    metadata: MetricMetadata,
}

impl OpenTelemetryRecorder {
    /// Creates a new OpenTelemetry recorder with the given meter.
    pub fn new(meter: Meter) -> Self {
        let metadata = MetricMetadata::new();
        let storage = OtelMetricStorage::new(meter, metadata.clone());
        Self { registry: Registry::new(storage), metadata }
    }

    /// Sets custom bucket boundaries for a histogram metric.
    ///
    /// Must be called before the histogram is first created. Boundaries cannot be
    /// changed after a histogram has been created.
    pub fn set_histogram_bounds(&self, key: &KeyName, bounds: Vec<f64>) {
        self.metadata.set_histogram_bounds(key.clone(), bounds);
    }

    /// Gets a description entry for testing purposes.
    #[cfg(test)]
    pub fn get_description(
        &self,
        key_name: KeyName,
        metric_kind: MetricKind,
    ) -> Option<crate::metadata::MetricDescription> {
        self.metadata.get_description(&key_name, metric_kind)
    }
}

impl Recorder for OpenTelemetryRecorder {
    fn describe_counter(
        &self,
        _key_name: KeyName,
        _unit: Option<Unit>,
        _description: SharedString,
    ) {
        self.metadata.set_description(_key_name, MetricKind::Counter, _unit, _description);
    }

    fn describe_gauge(&self, _key_name: KeyName, _unit: Option<Unit>, _description: SharedString) {
        self.metadata.set_description(_key_name, MetricKind::Gauge, _unit, _description);
    }

    fn describe_histogram(
        &self,
        _key_name: KeyName,
        _unit: Option<Unit>,
        _description: SharedString,
    ) {
        self.metadata.set_description(_key_name, MetricKind::Histogram, _unit, _description);
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
