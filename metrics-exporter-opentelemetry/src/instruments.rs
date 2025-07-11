//! OpenTelemetry instrument wrappers for metrics traits.

use metrics::{CounterFn, GaugeFn, HistogramFn};
use opentelemetry::metrics::{
    AsyncInstrumentBuilder, Histogram, ObservableCounter, ObservableGauge,
};
use opentelemetry::KeyValue;
use portable_atomic::{AtomicF64, Ordering};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

pub struct OtelCounter {
    #[allow(dead_code)] // prevent from drop
    counter: ObservableCounter<u64>,
    value: Arc<AtomicU64>,
}

impl OtelCounter {
    pub fn new(
        counter_builder: AsyncInstrumentBuilder<ObservableCounter<u64>, u64>,
        attributes: Vec<KeyValue>,
    ) -> Self {
        let value = Arc::new(AtomicU64::new(0));
        let value_moved = Arc::clone(&value);
        let otel_counter = counter_builder
            .with_callback(move |observer| {
                observer.observe(value_moved.load(Ordering::Relaxed), &attributes);
            })
            .build();
        Self { counter: otel_counter, value }
    }
}

impl CounterFn for OtelCounter {
    fn increment(&self, value: u64) {
        self.value.fetch_add(value, Ordering::Relaxed);
    }

    fn absolute(&self, value: u64) {
        self.value.store(value, Ordering::Relaxed);
    }
}

pub struct OtelGauge {
    #[allow(dead_code)] // prevent from drop
    gauge: ObservableGauge<f64>,
    value: Arc<AtomicF64>,
}

impl OtelGauge {
    pub fn new(
        gauge_builder: AsyncInstrumentBuilder<ObservableGauge<f64>, f64>,
        attributes: Vec<KeyValue>,
    ) -> Self {
        let value = Arc::new(AtomicF64::new(0.0));
        let value_moved = value.clone();
        let otel_gauge = gauge_builder
            .with_callback(move |observer| {
                observer.observe(value_moved.load(Ordering::Relaxed), &attributes);
            })
            .build();
        Self { gauge: otel_gauge, value }
    }
}

impl GaugeFn for OtelGauge {
    fn increment(&self, value: f64) {
        self.value.fetch_add(value, Ordering::Relaxed);
    }

    fn decrement(&self, value: f64) {
        self.value.fetch_sub(value, Ordering::Relaxed);
    }

    fn set(&self, value: f64) {
        self.value.store(value, Ordering::Relaxed);
    }
}

pub struct OtelHistogram {
    histogram: Histogram<f64>,
    attributes: Vec<KeyValue>,
}

impl OtelHistogram {
    pub fn new(histogram: Histogram<f64>, attributes: Vec<KeyValue>) -> Self {
        Self { histogram, attributes }
    }
}

impl HistogramFn for OtelHistogram {
    fn record(&self, value: f64) {
        self.histogram.record(value, &self.attributes);
    }
}
