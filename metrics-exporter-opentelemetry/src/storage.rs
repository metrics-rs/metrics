use crate::description::{DescriptionEntry, DescriptionTable};
use crate::instruments::{OtelCounter, OtelGauge, OtelHistogram};
use metrics::{Key, KeyName};
use metrics_util::registry::Storage;
use metrics_util::MetricKind;
use opentelemetry::metrics::{AsyncInstrumentBuilder, HistogramBuilder, Meter};
use opentelemetry::KeyValue;
use std::collections::HashMap;
use std::sync::{Arc, PoisonError, RwLock};

pub struct OtelMetricStorage {
    meter: Meter,
    description_table: Arc<DescriptionTable>,
    histogram_bounds: Arc<RwLock<HashMap<KeyName, Vec<f64>>>>,
}

impl OtelMetricStorage {
    pub fn new(meter: Meter, description_table: Arc<DescriptionTable>, histogram_bounds: Arc<RwLock<HashMap<KeyName, Vec<f64>>>>) -> Self {
        Self { 
            meter, 
            description_table,
            histogram_bounds,
        }
    }

    fn get_attributes(key: &Key) -> Vec<KeyValue> {
        key.labels()
            .map(|label| KeyValue::new(label.key().to_string(), label.value().to_string()))
            .collect()
    }

    fn with_description_entry<'a, I, M>(
        description_entry: &DescriptionEntry,
        builder: AsyncInstrumentBuilder<'a, I, M>,
    ) -> AsyncInstrumentBuilder<'a, I, M> {
        // let builder = builder.with_description(self.description.to_string());
        match description_entry.unit() {
            Some(unit) => builder
                .with_description(description_entry.description().to_string())
                .with_unit(unit.as_canonical_label()),
            None => builder.with_description(description_entry.description().to_string()),
        }
    }
    fn with_description_entry_histogram<'a, T>(
        description_entry: &DescriptionEntry,
        builder: HistogramBuilder<'a, T>,
    ) -> HistogramBuilder<'a, T> {
        // let builder = builder.with_description(self.description.to_string());
        match description_entry.unit() {
            Some(unit) => builder
                .with_description(description_entry.description().to_string())
                .with_unit(unit.as_canonical_label()),
            None => builder.with_description(description_entry.description().to_string()),
        }
    }
}

impl Storage<Key> for OtelMetricStorage {
    type Counter = Arc<OtelCounter>;
    type Gauge = Arc<OtelGauge>;
    type Histogram = Arc<OtelHistogram>;

    fn counter(&self, key: &Key) -> Self::Counter {
        let builder = self.meter.u64_observable_counter(key.name().to_string());
        let description = self
            .description_table
            .get_describe(KeyName::from(key.name().to_string()), MetricKind::Counter);
        let builder = if let Some(description) = description {
            Self::with_description_entry(&description, builder)
        } else {
            builder
        };
        let attributes = Self::get_attributes(key);
        Arc::new(OtelCounter::new(builder, attributes))
    }

    fn gauge(&self, key: &Key) -> Self::Gauge {
        let builder = self.meter.f64_observable_gauge(key.name().to_string());
        let description = self
            .description_table
            .get_describe(KeyName::from(key.name().to_string()), MetricKind::Gauge);
        let builder = if let Some(description) = description {
            Self::with_description_entry(&description, builder)
        } else {
            builder
        };
        let attributes = Self::get_attributes(key);
        Arc::new(OtelGauge::new(builder, attributes))
    }

    fn histogram(&self, key: &Key) -> Self::Histogram {
        let builder = self.meter.f64_histogram(key.name().to_string());
        let description = self
            .description_table
            .get_describe(KeyName::from(key.name().to_string()), MetricKind::Histogram);
        let builder = if let Some(description) = description {
            Self::with_description_entry_histogram(&description, builder)
        } else {
            builder
        };
        
        // Apply histogram bounds if they exist
        let key_name = KeyName::from(key.name().to_string());
        let builder = {
            let bounds_map = self.histogram_bounds.read().unwrap_or_else(PoisonError::into_inner);;
            if let Some(bounds) = bounds_map.get(&key_name) {
                builder.with_boundaries(bounds.clone())
            } else {
                builder
            }
        };
        
        let attributes = Self::get_attributes(key);
        Arc::new(OtelHistogram::new(builder.build(), attributes))
    }
}
