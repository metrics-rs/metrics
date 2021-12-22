use crate::layers::Layer;
use metrics::{Counter, Gauge, Histogram, Key, Recorder, SharedString, Unit};

/// Applies a prefix to every metric key.
///
/// Keys will be prefixed in the format of `<prefix>.<remaining>`.
pub struct Prefix<R> {
    prefix: SharedString,
    inner: R,
}

impl<R> Prefix<R> {
    fn prefix_key(&self, key: &Key) -> Key {
        let mut new_name = String::with_capacity(self.prefix.len() + 1 + key.name().len());
        new_name.push_str(self.prefix.as_ref());
        new_name.push('.');
        new_name.push_str(key.name());

        Key::from_parts(new_name, key.labels())
    }
}

impl<R: Recorder> Recorder for Prefix<R> {
    fn describe_counter(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        let new_key = self.prefix_key(key);
        self.inner.describe_counter(&new_key, unit, description)
    }

    fn describe_gauge(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        let new_key = self.prefix_key(key);
        self.inner.describe_gauge(&new_key, unit, description)
    }

    fn describe_histogram(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        let new_key = self.prefix_key(key);
        self.inner.describe_histogram(&new_key, unit, description)
    }

    fn register_counter(&self, key: &Key) -> Counter {
        let new_key = self.prefix_key(key);
        self.inner.register_counter(&new_key)
    }

    fn register_gauge(&self, key: &Key) -> Gauge {
        let new_key = self.prefix_key(key);
        self.inner.register_gauge(&new_key)
    }

    fn register_histogram(&self, key: &Key) -> Histogram {
        let new_key = self.prefix_key(key);
        self.inner.register_histogram(&new_key)
    }
}

/// A layer for applying a prefix to every metric key.
///
/// More information on the behavior of the layer can be found in [`Prefix`].
pub struct PrefixLayer(&'static str);

impl PrefixLayer {
    /// Creates a new `PrefixLayer` based on the given prefix.
    pub fn new<S: Into<String>>(prefix: S) -> PrefixLayer {
        PrefixLayer(Box::leak(prefix.into().into_boxed_str()))
    }
}

impl<R> Layer<R> for PrefixLayer {
    type Output = Prefix<R>;

    fn layer(&self, inner: R) -> Self::Output {
        Prefix {
            prefix: self.0.into(),
            inner,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PrefixLayer;
    use crate::layers::Layer;
    use crate::test_util::*;
    use metrics::{Counter, Gauge, Histogram, Unit};

    #[test]
    fn test_basic_functionality() {
        let inputs = vec![
            RecorderOperation::DescribeCounter(
                "counter_key".into(),
                Some(Unit::Count),
                Some("counter desc"),
            ),
            RecorderOperation::DescribeGauge(
                "gauge_key".into(),
                Some(Unit::Bytes),
                Some("gauge desc"),
            ),
            RecorderOperation::DescribeHistogram(
                "histogram_key".into(),
                Some(Unit::Nanoseconds),
                Some("histogram desc"),
            ),
            RecorderOperation::RegisterCounter("counter_key".into(), Counter::noop()),
            RecorderOperation::RegisterGauge("gauge_key".into(), Gauge::noop()),
            RecorderOperation::RegisterHistogram("histogram_key".into(), Histogram::noop()),
        ];

        let expectations = vec![
            RecorderOperation::DescribeCounter(
                "testing.counter_key".into(),
                Some(Unit::Count),
                Some("counter desc"),
            ),
            RecorderOperation::DescribeGauge(
                "testing.gauge_key".into(),
                Some(Unit::Bytes),
                Some("gauge desc"),
            ),
            RecorderOperation::DescribeHistogram(
                "testing.histogram_key".into(),
                Some(Unit::Nanoseconds),
                Some("histogram desc"),
            ),
            RecorderOperation::RegisterCounter("testing.counter_key".into(), Counter::noop()),
            RecorderOperation::RegisterGauge("testing.gauge_key".into(), Gauge::noop()),
            RecorderOperation::RegisterHistogram("testing.histogram_key".into(), Histogram::noop()),
        ];

        let recorder = MockBasicRecorder::from_operations(expectations);
        let prefix = PrefixLayer::new("testing");
        let prefix = prefix.layer(recorder);

        for operation in inputs {
            operation.apply_to_recorder(&prefix);
        }
    }
}
