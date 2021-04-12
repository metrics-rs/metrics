use metrics::{GaugeValue, Key, Recorder, Unit};

/// Fans out metrics to multiple recorders.
pub struct Fanout {
    recorders: Vec<Box<dyn Recorder>>,
}

impl Recorder for Fanout {
    fn register_counter(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        for recorder in &self.recorders {
            recorder.register_counter(key, unit.clone(), description);
        }
    }

    fn register_gauge(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        for recorder in &self.recorders {
            recorder.register_gauge(key, unit.clone(), description);
        }
    }

    fn register_histogram(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        for recorder in &self.recorders {
            recorder.register_histogram(key, unit.clone(), description);
        }
    }

    fn increment_counter(&self, key: &Key, value: u64) {
        for recorder in &self.recorders {
            recorder.increment_counter(key, value);
        }
    }

    fn update_gauge(&self, key: &Key, value: GaugeValue) {
        for recorder in &self.recorders {
            recorder.update_gauge(key, value.clone());
        }
    }

    fn record_histogram(&self, key: &Key, value: f64) {
        for recorder in &self.recorders {
            recorder.record_histogram(key, value);
        }
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
    use metrics::{GaugeValue, Recorder, Unit};

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

        let before1 = snapshotter1.snapshot();
        let before2 = snapshotter2.snapshot();
        assert_eq!(before1.len(), 0);
        assert_eq!(before2.len(), 0);

        let ud = &[(Unit::Count, "counter desc"), (Unit::Bytes, "gauge desc")];

        fanout.register_counter(&tlkey, Some(ud[0].0.clone()), Some(ud[0].1));
        fanout.register_gauge(&hsbkey, Some(ud[1].0.clone()), Some(ud[1].1));
        fanout.increment_counter(&tlkey, 47);
        fanout.update_gauge(&hsbkey, GaugeValue::Absolute(12.0));

        let after1 = snapshotter1.snapshot();
        let after2 = snapshotter2.snapshot();
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
