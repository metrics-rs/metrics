use std::sync::Arc;
use opentelemetry::KeyValue;
use opentelemetry::metrics::Meter;
use metrics::Key;
use metrics_util::registry::Storage;
use crate::instruments::{OtelCounter, OtelGauge, OtelHistogram};

pub struct OtelMetricStorage {
    meter: Meter,
}

impl OtelMetricStorage {
    pub fn new(meter: Meter) -> Self {
        Self { meter }
    }

    fn get_attributes(key: &Key) -> Vec<KeyValue> {
        key.labels()
            .map(|label| KeyValue::new(label.key().to_string(), label.value().to_string()))
            .collect()
    }
}

impl Storage<Key> for OtelMetricStorage {
    type Counter = Arc<OtelCounter>;
    type Gauge = Arc<OtelGauge>;
    type Histogram = Arc<OtelHistogram>;

    fn counter(&self, key: &Key) -> Self::Counter {
        let otel_counter_builder = self
            .meter
            .u64_observable_counter(key.name().to_string());
        let attributes = Self::get_attributes(key);
        Arc::new(OtelCounter::new(otel_counter_builder, attributes))
    }

    fn gauge(&self, key: &Key) -> Self::Gauge {
        let builder = self
            .meter
            .f64_observable_gauge(key.name().to_string());
        let attributes = Self::get_attributes(key);
        Arc::new(OtelGauge::new(builder, attributes))
    }

    fn histogram(&self, key: &Key) -> Self::Histogram {
        let histogram = self
            .meter
            .f64_histogram(key.name().to_string())
            .build();
        let attributes = Self::get_attributes(key);
        Arc::new(OtelHistogram::new(histogram, attributes))

    }
}
