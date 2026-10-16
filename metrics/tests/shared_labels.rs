use std::sync::{Arc, Mutex};

use metrics::{
    counter, gauge, histogram, Counter, Gauge, Histogram, IntoLabels, Key, KeyName, Label,
    Metadata, Recorder, SharedLabels, SharedString, Unit,
};

#[derive(Default)]
struct RetainingRecorder(Mutex<Vec<Key>>);

impl Recorder for RetainingRecorder {
    fn describe_counter(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
    fn describe_gauge(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
    fn describe_histogram(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

    fn register_counter(&self, key: &Key, _: &Metadata<'_>) -> Counter {
        self.0.lock().unwrap().push(key.to_retained());
        Counter::noop()
    }

    fn register_gauge(&self, key: &Key, _: &Metadata<'_>) -> Gauge {
        self.0.lock().unwrap().push(key.to_retained());
        Gauge::noop()
    }

    fn register_histogram(&self, key: &Key, _: &Metadata<'_>) -> Histogram {
        self.0.lock().unwrap().push(key.to_retained());
        Histogram::noop()
    }
}

#[test]
fn macros_share_one_label_array_across_instruments() {
    let labels: Arc<[Label]> = vec![Label::new("label_a", "value_a")].into();
    let weak = Arc::downgrade(&labels);
    let recorder = RetainingRecorder::default();

    metrics::with_local_recorder(&recorder, || {
        let _ = counter!("counter_a", Arc::clone(&labels));
        let _ = counter!("counter_b", Arc::clone(&labels));
        let _ = gauge!("gauge_a", Arc::clone(&labels));
        let _ = histogram!("histogram_a", SharedLabels::from(Arc::clone(&labels)));
    });

    {
        let keys = recorder.0.lock().unwrap();
        assert_eq!(keys.len(), 4);
        for key in keys.iter() {
            assert_eq!(key.labels().as_slice().as_ptr(), labels.as_ptr());
        }
    }
    drop(labels);
    assert!(weak.upgrade().is_some());
    drop(recorder);
    assert!(weak.upgrade().is_none());
}

#[test]
fn existing_custom_label_conversions_still_work() {
    struct CustomLabels;

    impl IntoLabels for CustomLabels {
        fn into_labels(self) -> Vec<Label> {
            vec![Label::new("label_a", "value_a")]
        }
    }

    let recorder = RetainingRecorder::default();
    metrics::with_local_recorder(&recorder, || {
        let _ = counter!("counter_a", CustomLabels);
    });

    let keys = recorder.0.lock().unwrap();
    assert_eq!(keys[0], Key::from_parts("counter_a", vec![Label::new("label_a", "value_a")]));
}
