use crate::layers::Layer;
use metrics::{GaugeValue, Key, Recorder, SharedString, Unit};

/// Applies a prefix to every metric key.
///
/// Keys will be prefixed in the format of `<prefix>.<remaining>`.
pub struct Prefix<R> {
    prefix: SharedString,
    inner: R,
}

impl<R> Prefix<R> {
    fn prefix_key(&self, key: &Key) -> Key {
        key.clone().prepend_name(self.prefix.clone()).into()
    }
}

impl<R: Recorder> Recorder for Prefix<R> {
    fn register_counter(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        let new_key = self.prefix_key(key);
        self.inner.register_counter(&new_key, unit, description)
    }

    fn register_gauge(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        let new_key = self.prefix_key(key);
        self.inner.register_gauge(&new_key, unit, description)
    }

    fn register_histogram(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        let new_key = self.prefix_key(key);
        self.inner.register_histogram(&new_key, unit, description)
    }

    fn increment_counter(&self, key: &Key, value: u64) {
        let new_key = self.prefix_key(key);
        self.inner.increment_counter(&new_key, value);
    }

    fn update_gauge(&self, key: &Key, value: GaugeValue) {
        let new_key = self.prefix_key(key);
        self.inner.update_gauge(&new_key, value);
    }

    fn record_histogram(&self, key: &Key, value: f64) {
        let new_key = self.prefix_key(key);
        self.inner.record_histogram(&new_key, value);
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

        let before = snapshotter.snapshot();
        assert_eq!(before.len(), 0);

        let ud = &[
            (Unit::Nanoseconds, "counter desc"),
            (Unit::Microseconds, "gauge desc"),
            (Unit::Milliseconds, "histogram desc"),
        ];

        layered.register_counter(&ckey, Some(ud[0].0.clone()), Some(ud[0].1));
        layered.register_gauge(&gkey, Some(ud[1].0.clone()), Some(ud[1].1));
        layered.register_histogram(&hkey, Some(ud[2].0.clone()), Some(ud[2].1));

        let after = snapshotter.snapshot();
        assert_eq!(after.len(), 3);

        for (i, (key, unit, desc, _value)) in after.iter().enumerate() {
            assert!(key.key().name().to_string().starts_with("testing"));
            assert_eq!(&Some(ud[i].0.clone()), unit);
            assert_eq!(&Some(ud[i].1), desc);
        }
    }
}
