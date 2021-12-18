use std::sync::Arc;

use metrics::{Counter, CounterFn, Gauge, GaugeFn, Histogram, HistogramFn, Key, Recorder, Unit};

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
    fn describe_counter(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        for recorder in &self.recorders {
            recorder.describe_counter(key, unit.clone(), description);
        }
    }

    fn describe_gauge(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        for recorder in &self.recorders {
            recorder.describe_gauge(key, unit.clone(), description);
        }
    }

    fn describe_histogram(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        for recorder in &self.recorders {
            recorder.describe_histogram(key, unit.clone(), description);
        }
    }

    fn register_counter(&self, key: &Key) -> Counter {
        let counters = self
            .recorders
            .iter()
            .map(|recorder| recorder.register_counter(key))
            .collect();

        FanoutCounter::from_counters(counters).into()
    }

    fn register_gauge(&self, key: &Key) -> Gauge {
        let gauges = self
            .recorders
            .iter()
            .map(|recorder| recorder.register_gauge(key))
            .collect();

        FanoutGauge::from_gauges(gauges).into()
    }

    fn register_histogram(&self, key: &Key) -> Histogram {
        let histograms = self
            .recorders
            .iter()
            .map(|recorder| recorder.register_histogram(key))
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
        Fanout {
            recorders: self.recorders,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FanoutBuilder;
    use crate::debugging::DebuggingRecorder;
    use metrics::{Recorder, Unit};

    #[test]
    fn test_basic_functionality() {
        let recorder1 = DebuggingRecorder::new();
        let snapshotter1 = recorder1.snapshotter();
        let recorder2 = DebuggingRecorder::new();
        let snapshotter2 = recorder2.snapshotter();
        let fanout = FanoutBuilder::default()
            .add_recorder(recorder1)
            .add_recorder(recorder2)
            .build();

        let tlkey = "tokio.loops".into();
        let hsbkey = "hyper.sent.bytes".into();

        let before1 = snapshotter1.snapshot().into_vec();
        let before2 = snapshotter2.snapshot().into_vec();
        assert_eq!(before1.len(), 0);
        assert_eq!(before2.len(), 0);

        let ud = &[(Unit::Count, "counter desc"), (Unit::Bytes, "gauge desc")];

        fanout.describe_counter(&tlkey, Some(ud[0].0.clone()), Some(ud[0].1));
        fanout.describe_gauge(&hsbkey, Some(ud[1].0.clone()), Some(ud[1].1));

        let counter = fanout.register_counter(&tlkey);
        counter.increment(47);
        let gauge = fanout.register_gauge(&hsbkey);
        gauge.set(12.0);

        let after1 = snapshotter1.snapshot().into_vec();
        let after2 = snapshotter2.snapshot().into_vec();
        assert_eq!(after1.len(), 2);
        assert_eq!(after2.len(), 2);

        let after = after1
            .into_iter()
            .zip(after2)
            .enumerate()
            .collect::<Vec<_>>();

        for (i, ((k1, u1, d1, v1), (k2, u2, d2, v2))) in after {
            assert_eq!(k1, k2);
            assert_eq!(u1, u2);
            assert_eq!(d1, d2);
            assert_eq!(v1, v2);
            assert_eq!(Some(ud[i].0.clone()), u1);
            assert_eq!(Some(ud[i].0.clone()), u2);
            assert_eq!(Some(ud[i].1), d1);
            assert_eq!(Some(ud[i].1), d2);
        }
    }
}
