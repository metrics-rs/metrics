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
    use crate::debugging::DebuggingRecorder;
    use crate::layers::Layer;
    use metrics::{Recorder, Unit};

    #[test]
    fn test_basic_functionality() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let prefix = PrefixLayer::new("testing");
        let layered = prefix.layer(recorder);

        let ckey = "counter_metric".into();
        let gkey = "gauge_metric".into();
        let hkey = "histogram_metric".into();

        let before = snapshotter.snapshot().into_vec();
        assert_eq!(before.len(), 0);

        let ud = &[
            (Unit::Nanoseconds, "counter desc"),
            (Unit::Microseconds, "gauge desc"),
            (Unit::Milliseconds, "histogram desc"),
        ];

        let _ = layered.describe_counter(&ckey, Some(ud[0].0.clone()), Some(ud[0].1));
        let _ = layered.register_counter(&ckey);
        let _ = layered.describe_gauge(&gkey, Some(ud[1].0.clone()), Some(ud[1].1));
        let _ = layered.register_gauge(&gkey);
        let _ = layered.describe_histogram(&hkey, Some(ud[2].0.clone()), Some(ud[2].1));
        let _ = layered.register_histogram(&hkey);

        let after = snapshotter.snapshot().into_vec();
        assert_eq!(after.len(), 3);

        for (i, (key, unit, desc, _value)) in after.into_iter().enumerate() {
            assert!(key.key().name().to_string().starts_with("testing"));
            assert_eq!(Some(ud[i].0.clone()), unit);
            assert_eq!(Some(ud[i].1), desc);
        }
    }
}
