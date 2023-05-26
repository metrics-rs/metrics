use std::sync::Arc;

use metrics::{
    Counter, CounterFn, Gauge, GaugeFn, Histogram, HistogramFn, Key, KeyName, Metadata, Recorder,
    SharedString, Unit,
};

struct FanoutCounter {
    counters: Vec<Counter>,
}

impl FanoutCounter {
    pub fn from_counters(counters: Vec<Counter>) -> Self {
        Self { counters }
    }
}

impl CounterFn for FanoutCounter {
    fn increment(&self, value: u64) {
        for counter in &self.counters {
            counter.increment(value);
        }
    }

    fn absolute(&self, value: u64) {
        for counter in &self.counters {
            counter.absolute(value);
        }
    }
}

impl From<FanoutCounter> for Counter {
    fn from(counter: FanoutCounter) -> Counter {
        Counter::from_arc(Arc::new(counter))
    }
}

struct FanoutGauge {
    gauges: Vec<Gauge>,
}

impl FanoutGauge {
    pub fn from_gauges(gauges: Vec<Gauge>) -> Self {
        Self { gauges }
    }
}

impl GaugeFn for FanoutGauge {
    fn increment(&self, value: f64) {
        for gauge in &self.gauges {
            gauge.increment(value);
        }
    }

    fn decrement(&self, value: f64) {
        for gauge in &self.gauges {
            gauge.decrement(value);
        }
    }

    fn set(&self, value: f64) {
        for gauge in &self.gauges {
            gauge.set(value);
        }
    }
}

impl From<FanoutGauge> for Gauge {
    fn from(gauge: FanoutGauge) -> Gauge {
        Gauge::from_arc(Arc::new(gauge))
    }
}

struct FanoutHistogram {
    histograms: Vec<Histogram>,
}

impl FanoutHistogram {
    pub fn from_histograms(histograms: Vec<Histogram>) -> Self {
        Self { histograms }
    }
}

impl HistogramFn for FanoutHistogram {
    fn record(&self, value: f64) {
        for histogram in &self.histograms {
            histogram.record(value);
        }
    }
}

impl From<FanoutHistogram> for Histogram {
    fn from(histogram: FanoutHistogram) -> Histogram {
        Histogram::from_arc(Arc::new(histogram))
    }
}

/// Fans out metrics to multiple recorders.
pub struct Fanout {
    recorders: Vec<Box<dyn Recorder>>,
}

impl Recorder for Fanout {
    fn describe_counter(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        for recorder in &self.recorders {
            recorder.describe_counter(key_name.clone(), unit, description.clone());
        }
    }

    fn describe_gauge(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        for recorder in &self.recorders {
            recorder.describe_gauge(key_name.clone(), unit, description.clone());
        }
    }

    fn describe_histogram(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        for recorder in &self.recorders {
            recorder.describe_histogram(key_name.clone(), unit, description.clone());
        }
    }

    fn register_counter(&self, key: &Key, metadata: &Metadata<'_>) -> Counter {
        let counters = self
            .recorders
            .iter()
            .map(|recorder| recorder.register_counter(key, metadata))
            .collect();

        FanoutCounter::from_counters(counters).into()
    }

    fn register_gauge(&self, key: &Key, metadata: &Metadata<'_>) -> Gauge {
        let gauges =
            self.recorders.iter().map(|recorder| recorder.register_gauge(key, metadata)).collect();

        FanoutGauge::from_gauges(gauges).into()
    }

    fn register_histogram(&self, key: &Key, metadata: &Metadata<'_>) -> Histogram {
        let histograms = self
            .recorders
            .iter()
            .map(|recorder| recorder.register_histogram(key, metadata))
            .collect();

        FanoutHistogram::from_histograms(histograms).into()
    }
}

/// A layer for fanning out metrics to multiple recorders.
///
/// More information on the behavior of the layer can be found in [`Fanout`].
#[derive(Default)]
pub struct FanoutBuilder {
    recorders: Vec<Box<dyn Recorder>>,
}

impl FanoutBuilder {
    /// Adds a recorder to the fanout list.
    pub fn add_recorder<R>(mut self, recorder: R) -> FanoutBuilder
    where
        R: Recorder + 'static,
    {
        self.recorders.push(Box::new(recorder));
        self
    }

    /// Builds the `Fanout` layer.
    pub fn build(self) -> Fanout {
        Fanout { recorders: self.recorders }
    }
}

#[cfg(test)]
mod tests {
    use super::FanoutBuilder;
    use crate::test_util::*;
    use metrics::{Counter, Gauge, Histogram, Unit};

    static METADATA: metrics::Metadata =
        metrics::Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));

    #[test]
    fn test_basic_functionality() {
        let operations = vec![
            RecorderOperation::DescribeCounter(
                "counter_key".into(),
                Some(Unit::Count),
                "counter desc".into(),
            ),
            RecorderOperation::DescribeGauge(
                "gauge_key".into(),
                Some(Unit::Bytes),
                "gauge desc".into(),
            ),
            RecorderOperation::DescribeHistogram(
                "histogram_key".into(),
                Some(Unit::Nanoseconds),
                "histogram desc".into(),
            ),
            RecorderOperation::RegisterCounter("counter_key".into(), Counter::noop(), &METADATA),
            RecorderOperation::RegisterGauge("gauge_key".into(), Gauge::noop(), &METADATA),
            RecorderOperation::RegisterHistogram(
                "histogram_key".into(),
                Histogram::noop(),
                &METADATA,
            ),
        ];

        let recorder1 = MockBasicRecorder::from_operations(operations.clone());
        let recorder2 = MockBasicRecorder::from_operations(operations.clone());
        let fanout =
            FanoutBuilder::default().add_recorder(recorder1).add_recorder(recorder2).build();

        for operation in operations {
            operation.apply_to_recorder(&fanout);
        }
    }
}
