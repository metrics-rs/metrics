use crate::instruments::{OtelCounter, OtelGauge, OtelHistogram};
use crate::metadata::{MetricDescription, MetricMetadata};
use metrics::{Key, KeyName};
use metrics_util::registry::Storage;
use metrics_util::MetricKind;
use opentelemetry::metrics::{AsyncInstrumentBuilder, HistogramBuilder, Meter};
use opentelemetry::KeyValue;
use std::borrow::Cow;
use std::sync::Arc;

pub struct OtelMetricStorage {
    meter: Meter,
    metadata: MetricMetadata,
}

impl OtelMetricStorage {
    pub fn new(meter: Meter, metadata: MetricMetadata) -> Self {
        Self { meter, metadata }
    }

    fn get_attributes(key: &Key) -> Vec<KeyValue> {
        key.labels()
            .map(|label| {
                let (key, value) = label.clone().into_parts();
                let key: Cow<'static, str> = key.into();
                let label: Cow<'static, str> = value.into();
                KeyValue::new(key, label)
            })
            .collect()
    }

    fn with_description<'a, I, M>(
        description: &MetricDescription,
        builder: AsyncInstrumentBuilder<'a, I, M>,
    ) -> AsyncInstrumentBuilder<'a, I, M> {
        match description.unit() {
            Some(unit) => {
                builder.with_description(description.description()).with_unit(unit.as_ucum_label())
            }
            None => builder.with_description(description.description()),
        }
    }

    fn with_description_histogram<'a, T>(
        description: &MetricDescription,
        builder: HistogramBuilder<'a, T>,
    ) -> HistogramBuilder<'a, T> {
        match description.unit() {
            Some(unit) => {
                builder.with_description(description.description()).with_unit(unit.as_ucum_label())
            }
            None => builder.with_description(description.description()),
        }
    }
}

impl Storage<Key> for OtelMetricStorage {
    type Counter = Arc<OtelCounter>;
    type Gauge = Arc<OtelGauge>;
    type Histogram = Arc<OtelHistogram>;

    fn counter(&self, key: &Key) -> Self::Counter {
        let key_name = key.name_shared().clone().into_inner();
        let builder = self.meter.u64_observable_counter(key_name.clone());
        let key_name = KeyName::from(key_name);
        let builder = if let Some(description) =
            self.metadata.get_description(&key_name, MetricKind::Counter)
        {
            Self::with_description(&description, builder)
        } else {
            builder
        };
        let attributes = Self::get_attributes(key);
        Arc::new(OtelCounter::new(builder, attributes))
    }

    fn gauge(&self, key: &Key) -> Self::Gauge {
        let key_name = key.name_shared().clone().into_inner();
        let builder = self.meter.f64_observable_gauge(key_name.clone());
        let key_name = KeyName::from(key_name);
        let builder = if let Some(description) =
            self.metadata.get_description(&key_name, MetricKind::Gauge)
        {
            Self::with_description(&description, builder)
        } else {
            builder
        };
        let attributes = Self::get_attributes(key);
        Arc::new(OtelGauge::new(builder, attributes))
    }

    fn histogram(&self, key: &Key) -> Self::Histogram {
        let key_name = key.name_shared().clone().into_inner();
        let builder = self.meter.f64_histogram(key_name.clone());
        let key_name = KeyName::from(key_name);

        let builder = if let Some(description) =
            self.metadata.get_description(&key_name, MetricKind::Histogram)
        {
            Self::with_description_histogram(&description, builder)
        } else {
            builder
        };

        // Apply histogram bounds if they exist
        let builder = if let Some(bounds) = self.metadata.get_histogram_bounds(&key_name) {
            builder.with_boundaries(bounds)
        } else {
            builder
        };

        let attributes = Self::get_attributes(key);
        Arc::new(OtelHistogram::new(builder.build(), attributes))
    }
}
