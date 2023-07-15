use crate::layers::Layer;
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};

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

    fn prefix_key_name(&self, key_name: KeyName) -> KeyName {
        let mut new_name = String::with_capacity(self.prefix.len() + 1 + key_name.as_str().len());
        new_name.push_str(self.prefix.as_ref());
        new_name.push('.');
        new_name.push_str(key_name.as_str());

        KeyName::from(new_name)
    }
}

impl<R: Recorder> Recorder for Prefix<R> {
    fn describe_counter(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        let new_key_name = self.prefix_key_name(key_name);
        self.inner.describe_counter(new_key_name, unit, description)
    }

    fn describe_gauge(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        let new_key_name = self.prefix_key_name(key_name);
        self.inner.describe_gauge(new_key_name, unit, description)
    }

    fn describe_histogram(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        let new_key_name = self.prefix_key_name(key_name);
        self.inner.describe_histogram(new_key_name, unit, description)
    }

    fn register_counter(&self, key: &Key, metadata: &Metadata<'_>) -> Counter {
        let new_key = self.prefix_key(key);
        self.inner.register_counter(&new_key, metadata)
    }

    fn register_gauge(&self, key: &Key, metadata: &Metadata<'_>) -> Gauge {
        let new_key = self.prefix_key(key);
        self.inner.register_gauge(&new_key, metadata)
    }

    fn register_histogram(&self, key: &Key, metadata: &Metadata<'_>) -> Histogram {
        let new_key = self.prefix_key(key);
        self.inner.register_histogram(&new_key, metadata)
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
        Prefix { prefix: self.0.into(), inner }
    }
}

#[cfg(test)]
mod tests {
    use super::{Prefix, PrefixLayer};
    use crate::layers::Layer;
    use crate::test_util::*;
    use metrics::{Counter, Gauge, Histogram, Key, KeyName, Unit};

    static METADATA: metrics::Metadata =
        metrics::Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));

    #[test]
    fn test_basic_functionality() {
        let inputs = vec![
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

        let expectations = vec![
            RecorderOperation::DescribeCounter(
                "testing.counter_key".into(),
                Some(Unit::Count),
                "counter desc".into(),
            ),
            RecorderOperation::DescribeGauge(
                "testing.gauge_key".into(),
                Some(Unit::Bytes),
                "gauge desc".into(),
            ),
            RecorderOperation::DescribeHistogram(
                "testing.histogram_key".into(),
                Some(Unit::Nanoseconds),
                "histogram desc".into(),
            ),
            RecorderOperation::RegisterCounter(
                "testing.counter_key".into(),
                Counter::noop(),
                &METADATA,
            ),
            RecorderOperation::RegisterGauge("testing.gauge_key".into(), Gauge::noop(), &METADATA),
            RecorderOperation::RegisterHistogram(
                "testing.histogram_key".into(),
                Histogram::noop(),
                &METADATA,
            ),
        ];

        let recorder = MockBasicRecorder::from_operations(expectations);
        let prefix = PrefixLayer::new("testing");
        let prefix = prefix.layer(recorder);

        for operation in inputs {
            operation.apply_to_recorder(&prefix);
        }
    }

    #[test]
    fn test_key_vs_key_name() {
        let prefix = Prefix { prefix: "foobar".into(), inner: () };

        let key_name = KeyName::from("my_key");
        let key = Key::from_name(key_name.clone());

        let prefixed_key = prefix.prefix_key(&key);
        let prefixed_key_name = prefix.prefix_key_name(key_name);

        assert_eq!(
            prefixed_key.name(),
            prefixed_key_name.as_str(),
            "prefixed key and prefixed key name should match"
        );
    }
}
