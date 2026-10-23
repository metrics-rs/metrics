use std::sync::{Arc, Mutex};

use metrics::{
    counter, gauge, histogram, Counter, Gauge, Histogram, IntoLabels, Key, KeyName, Label,
    LabelSet, Metadata, Recorder, SharedLabels, SharedString, Unit,
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

/// All metric macros retain the same prefix and leaf storage after their owners are dropped.
#[test]
fn macros_share_label_prefixes_across_instruments() {
    let base: Arc<[Label]> = vec![Label::new("label_a", "value_a")].into();
    let weak = Arc::downgrade(&base);
    let prefix = metrics::labels!(Arc::clone(&base));
    let extended_labels = metrics::labels!(&prefix, "label_b" => "value_b");
    let nested_labels = metrics::labels!(&extended_labels, "label_c" => "value_c");
    let recorder = RetainingRecorder::default();
    metrics::with_local_recorder(&recorder, || {
        let _ = counter!("counter_a", &extended_labels);
        let _ = gauge!("gauge_a", &extended_labels);
        let _ = histogram!("histogram_a", &nested_labels);
    });
    {
        let keys = recorder.0.lock().unwrap();
        assert_eq!(keys.len(), 3);
        for key in keys.iter() {
            assert!(std::ptr::eq(key.labels().next().unwrap(), &base[0]));
            assert!(std::ptr::eq(
                key.labels().nth(1).unwrap(),
                extended_labels.iter().nth(1).unwrap()
            ));
        }
        assert_eq!(keys[2].labels().next_back().unwrap(), &Label::new("label_c", "value_c"));
    }
    drop(base);
    drop(prefix);
    drop(extended_labels);
    drop(nested_labels);
    assert!(weak.upgrade().is_some());
    drop(recorder);
    assert!(weak.upgrade().is_none());
}

/// Prefix boundaries and label order do not affect key identity, hashes, or map lookup.
#[test]
fn prefixed_keys_match_flat_keys() {
    use std::collections::{BTreeMap, HashMap};

    for count in [0, 1, 2, 7, 8, 16] {
        let flat: Vec<_> = (0..count).map(|i| Label::new(i.to_string(), "value")).collect();
        for split in 0..=count {
            let prefix = LabelSet::from(flat[..split].to_vec());
            let labels = prefix.with_extra_labels(flat[split..].to_vec());
            let key = Key::from_parts("metric_a", &labels);
            let reversed =
                Key::from_parts("metric_a", flat.iter().rev().cloned().collect::<Vec<_>>());
            assert_eq!(key, reversed);
            assert_eq!(key.get_hash(), reversed.get_hash());
            assert_eq!(key.cmp(&reversed), std::cmp::Ordering::Equal);
            let mut hash_map = HashMap::new();
            let mut btree_map = BTreeMap::new();
            hash_map.insert(key.clone(), 42);
            btree_map.insert(key, 42);
            assert_eq!(hash_map.get(&reversed), Some(&42));
            assert_eq!(btree_map.get(&reversed), Some(&42));
        }
    }
}

/// Repeated keys across prefix boundaries keep all values and remain order independent.
#[test]
fn repeated_label_keys_preserve_content_identity() {
    for count in [3, 7, 8, 16] {
        let flat: Vec<_> = (0..count).map(|i| Label::new("same_key", i.to_string())).collect();
        let labels = LabelSet::from(flat[..2].to_vec()).with_extra_labels(flat[2..].to_vec());
        let prefixed = Key::from_parts("metric_a", &labels);
        let reversed = Key::from_parts("metric_a", flat.iter().rev().cloned().collect::<Vec<_>>());
        assert_eq!(prefixed.labels().len(), count);
        assert_eq!(prefixed, reversed);
        assert_eq!(prefixed.cmp(&reversed), std::cmp::Ordering::Equal);
        assert_eq!(prefixed.get_hash(), reversed.get_hash());
    }
}

/// Inline label extensions work with every instrument, named options, and trailing commas.
#[test]
fn macros_accept_shared_prefixes_with_inline_labels() {
    let base: Arc<[Label]> = vec![Label::new("label_a", "value_a")].into();
    let labels = LabelSet::from(Arc::clone(&base));
    let recorder = RetainingRecorder::default();
    let evaluations = std::cell::Cell::new(0);
    metrics::with_local_recorder(&recorder, || {
        let _ = counter!(
            description: "Example counter",
            unit: Unit::Count,
            target: "example",
            level: metrics::Level::DEBUG,
            "counter_a",
            { evaluations.set(evaluations.get() + 1); &labels },
            "label_b" => String::from("value_b"),
            "label_c" => "value_c",
        );
        let _ = gauge!(format!("gauge_{}", "a"), &labels, "label_b" => "value_b");
        let _ = histogram!("histogram_a", SharedLabels::from(Arc::clone(&base)), "label_b" => "value_b",);
    });
    assert_eq!(evaluations.get(), 1);
    let keys = recorder.0.lock().unwrap();
    assert_eq!(keys.len(), 3);
    for key in keys.iter() {
        assert!(std::ptr::eq(key.labels().next().unwrap(), &base[0]));
        assert_eq!(key.labels().nth(1), Some(&Label::new("label_b", "value_b")));
    }
    assert_eq!(keys[0].labels().nth(2), Some(&Label::new("label_c", "value_c")));
}

/// Existing shared-label conversions and the exact slice iterator type remain supported.
#[test]
fn existing_shared_label_implementations_keep_their_storage() {
    struct CustomSharedLabels(Arc<[Label]>);

    impl IntoLabels for CustomSharedLabels {
        fn into_labels(self) -> Vec<Label> {
            panic!("the existing shared conversion must be used")
        }

        fn into_shared_labels(self) -> SharedLabels {
            self.0.into()
        }
    }

    let labels: Arc<[Label]> = vec![Label::new("label_a", "value_a")].into();
    let key = Key::from_parts("metric_a", CustomSharedLabels(Arc::clone(&labels)));
    let legacy = key.labels();
    assert_eq!(legacy.as_slice().as_ptr(), labels.as_ptr());
    let shared: SharedLabels = Arc::clone(&labels).into();
    let shared_iter: std::slice::Iter<'_, Label> = shared.iter();
    assert_eq!(shared_iter.as_slice().as_ptr(), labels.as_ptr());
    let cow: std::borrow::Cow<'static, [Label]> = shared.into();
    assert_eq!(cow.as_ref(), labels.as_ref());
}

/// All inputs remain shared across reusable sets, metric macros, and retained recorder keys.
#[test]
fn macros_combine_multiple_shared_sets() {
    let inputs: Vec<Arc<[Label]>> = (0..3)
        .map(|index| vec![Label::new(index.to_string(), String::from("value"))].into())
        .collect();
    let weak: Vec<_> = inputs.iter().map(Arc::downgrade).collect();
    let first = metrics::labels!(Arc::clone(&inputs[0]));
    let second = metrics::labels!(Arc::clone(&inputs[1]));
    let third = metrics::labels!(Arc::clone(&inputs[2]));
    let evaluations = std::cell::Cell::new(0);
    let combined = metrics::labels!(
        {
            evaluations.set(evaluations.get() + 1);
            &first
        },
        {
            evaluations.set(evaluations.get() + 1);
            &second
        },
        {
            evaluations.set(evaluations.get() + 1);
            &third
        },
    );
    assert_eq!(evaluations.get(), 3);
    let recorder = RetainingRecorder::default();
    metrics::with_local_recorder(&recorder, || {
        let _ = counter!("counter_a", &combined);
        let _ = gauge!(target: "example", level: metrics::Level::DEBUG, "gauge_a", &first, &second, &third,);
        let _ =
            histogram!(format!("histogram_{}", "a"), &first, &second, &third, "extra" => "value",);
    });
    {
        let keys = recorder.0.lock().unwrap();
        assert_eq!(keys.len(), 3);
        for key in keys.iter() {
            for (label, source) in key.labels().zip(&inputs) {
                assert!(std::ptr::eq(label, &source[0]));
            }
        }
        assert_eq!(keys[0].labels().len(), 3);
        assert_eq!(keys[1].labels().len(), 3);
        assert_eq!(keys[2].labels().len(), 4);
        assert_eq!(keys[2].labels().next_back(), Some(&Label::new("extra", "value")));
    }
    drop(combined);
    drop(first);
    drop(second);
    drop(third);
    drop(inputs);
    assert!(weak.iter().all(|weak| weak.upgrade().is_some()));
    drop(recorder);
    assert!(weak.iter().all(|weak| weak.upgrade().is_none()));
}

#[test]
fn comparisons_preserve_duplicate_multiplicity() {
    for count in [2, 7, 8, 16] {
        let mut labels = vec![Label::new("a", "1"); count - 1];
        labels.push(Label::new("b", "2"));
        let combined = LabelSet::from(labels[..1].to_vec()).with_extra_labels(labels[1..].to_vec());
        let key = Key::from_parts("metric", combined);
        labels.reverse();
        let reversed = Key::from_parts("metric", labels.clone());
        assert_eq!(key, reversed);
        assert_eq!(key.cmp(&reversed), std::cmp::Ordering::Equal);
        labels[0] = Label::new("a", "1");
        let different = Key::from_parts("metric", labels);
        assert_ne!(key, different);
        assert_ne!(key.cmp(&different), std::cmp::Ordering::Equal);
        assert_eq!(key.cmp(&different), different.cmp(&key).reverse());
    }
}
