use crate::common::{Identifier, Kind, Measurement, ValueHandle, ValueSnapshot};
use crate::config::Configuration;
use crate::data::Snapshot;
use crate::registry::ScopeRegistry;
use arc_swap::ArcSwap;
use im::hashmap::HashMap;
use metrics_core::Observer;
use quanta::Clock;
use std::sync::Arc;

#[derive(Debug)]
pub(crate) struct MetricRegistry {
    scope_registry: Arc<ScopeRegistry>,
    metrics: ArcSwap<HashMap<Identifier, ValueHandle>>,
    config: Configuration,
    clock: Clock,
}

impl MetricRegistry {
    pub fn new(scope_registry: Arc<ScopeRegistry>, config: Configuration, clock: Clock) -> Self {
        MetricRegistry {
            scope_registry,
            metrics: ArcSwap::new(Arc::new(HashMap::new())),
            config,
            clock,
        }
    }

    pub fn get_or_register(&self, id: Identifier) -> ValueHandle {
        loop {
            let old_metrics = self.metrics.load();
            match old_metrics.get(&id) {
                Some(handle) => return handle.clone(),
                None => {
                    let value_handle = match id.kind() {
                        Kind::Counter => ValueHandle::counter(),
                        Kind::Gauge => ValueHandle::gauge(),
                        Kind::Histogram => ValueHandle::histogram(
                            self.config.histogram_window,
                            self.config.histogram_granularity,
                            self.clock.clone(),
                        ),
                        Kind::Proxy => ValueHandle::proxy(),
                    };

                    let mut new_metrics = (**self.metrics.load()).clone();
                    match new_metrics.insert(id.clone(), value_handle.clone()) {
                        Some(other_value_handle) => {
                            // Somebody else beat us to it.
                            return other_value_handle;
                        }
                        None => {
                            let prev_metrics = self
                                .metrics
                                .compare_and_swap(&old_metrics, Arc::new(new_metrics));
                            if Arc::ptr_eq(&old_metrics, &prev_metrics) {
                                return value_handle;
                            }
                            // If we weren't able to cleanly update the map, then try again.
                        }
                    }
                }
            }
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut values = Vec::new();

        let metrics = (**self.metrics.load()).clone();
        for (id, value) in metrics.into_iter() {
            let (key, scope_handle, _) = id.into_parts();
            let scope = self.scope_registry.get(scope_handle);

            match value.snapshot() {
                ValueSnapshot::Single(measurement) => {
                    let key = key.map_name(|name| scope.into_string(name));
                    values.push((key, measurement));
                }
                ValueSnapshot::Multiple(mut measurements) => {
                    // Tack on the key name that this proxy was registered with to the scope so
                    // that we can clone _that_, and then scope our individual measurements.
                    let (base_key, labels) = key.into_parts();
                    let scope = scope.clone().add_part(base_key);

                    for (subkey, measurement) in measurements.drain(..) {
                        let scope = scope.clone();
                        let mut subkey = subkey.map_name(|name| scope.into_string(name));
                        subkey.add_labels(labels.clone());
                        values.push((subkey, measurement));
                    }
                }
            }
        }

        Snapshot::new(values)
    }

    pub fn observe<O: Observer>(&self, observer: &mut O) {
        let metrics = (**self.metrics.load()).clone();
        for (id, value) in metrics.into_iter() {
            let (key, scope_handle, _) = id.into_parts();
            let scope = self.scope_registry.get(scope_handle);

            let observe = |observer: &mut O, key, measurement| match measurement {
                Measurement::Counter(value) => observer.observe_counter(key, value),
                Measurement::Gauge(value) => observer.observe_gauge(key, value),
                Measurement::Histogram(stream) => stream.decompress_with(|values| {
                    observer.observe_histogram(key.clone(), values);
                }),
            };

            match value.snapshot() {
                ValueSnapshot::Single(measurement) => {
                    let key = key.map_name(|name| scope.into_string(name));
                    observe(observer, key, measurement);
                }
                ValueSnapshot::Multiple(mut measurements) => {
                    // Tack on the key name that this proxy was registered with to the scope so
                    // that we can clone _that_, and then scope our individual measurements.
                    let (base_key, labels) = key.into_parts();
                    let scope = scope.clone().add_part(base_key);

                    for (subkey, measurement) in measurements.drain(..) {
                        let scope = scope.clone();
                        let mut subkey = subkey.map_name(|name| scope.into_string(name));
                        subkey.add_labels(labels.clone());
                        observe(observer, subkey, measurement);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Clock, Configuration, Identifier, Kind, Measurement, MetricRegistry, ScopeRegistry,
    };
    use crate::data::{Counter, Gauge, Histogram};
    use metrics_core::{Key, Label};
    use metrics_util::StreamingIntegers;
    use std::mem;
    use std::sync::Arc;

    #[test]
    fn test_snapshot() {
        // Get our registry.
        let sr = Arc::new(ScopeRegistry::new());
        let config = Configuration::mock();
        let (clock, _) = Clock::mock();
        let mr = Arc::new(MetricRegistry::new(sr, config, clock));

        // Set some metrics.
        let cid = Identifier::new("counter", 0, Kind::Counter);
        let counter: Counter = mr.get_or_register(cid).into();
        counter.record(15);

        let gid = Identifier::new("gauge", 0, Kind::Gauge);
        let gauge: Gauge = mr.get_or_register(gid).into();
        gauge.record(89);

        let hid = Identifier::new("histogram", 0, Kind::Histogram);
        let histogram: Histogram = mr.get_or_register(hid).into();
        histogram.record_value(89);

        let pid = Identifier::new("proxy", 0, Kind::Proxy);
        let proxy = mr.get_or_register(pid);
        proxy.update_proxy(|| vec![(Key::from_name("counter"), Measurement::Counter(13))]);

        let mut snapshot = mr.snapshot().into_measurements();
        snapshot.sort_by_key(|(k, _)| k.name());

        let mut expected = vec![
            (Key::from_name("counter"), Measurement::Counter(15)),
            (Key::from_name("gauge"), Measurement::Gauge(89)),
            (
                Key::from_name("histogram"),
                Measurement::Histogram(StreamingIntegers::new()),
            ),
            (Key::from_name("proxy.counter"), Measurement::Counter(13)),
        ];
        expected.sort_by_key(|(k, _)| k.name());

        assert_eq!(snapshot.len(), expected.len());
        for rhs in expected {
            let lhs = snapshot.remove(0);
            assert_eq!(lhs.0, rhs.0);
            assert_eq!(mem::discriminant(&lhs.1), mem::discriminant(&rhs.1));
        }
    }

    #[test]
    fn test_snapshot_with_labels() {
        // Get our registry.
        let sr = Arc::new(ScopeRegistry::new());
        let config = Configuration::mock();
        let (clock, _) = Clock::mock();
        let mr = Arc::new(MetricRegistry::new(sr, config, clock));

        let labels = vec![Label::new("type", "test")];

        // Set some metrics.
        let cid = Identifier::new(("counter", labels.clone()), 0, Kind::Counter);
        let counter: Counter = mr.get_or_register(cid).into();
        counter.record(15);

        let gid = Identifier::new(("gauge", labels.clone()), 0, Kind::Gauge);
        let gauge: Gauge = mr.get_or_register(gid).into();
        gauge.record(89);

        let hid = Identifier::new(("histogram", labels.clone()), 0, Kind::Histogram);
        let histogram: Histogram = mr.get_or_register(hid).into();
        histogram.record_value(89);

        let pid = Identifier::new(("proxy", labels.clone()), 0, Kind::Proxy);
        let proxy = mr.get_or_register(pid);
        proxy.update_proxy(|| vec![(Key::from_name("counter"), Measurement::Counter(13))]);

        let mut snapshot = mr.snapshot().into_measurements();
        snapshot.sort_by_key(|(k, _)| k.name());

        let mut expected = vec![
            (
                Key::from_name_and_labels("counter", labels.clone()),
                Measurement::Counter(15),
            ),
            (
                Key::from_name_and_labels("gauge", labels.clone()),
                Measurement::Gauge(89),
            ),
            (
                Key::from_name_and_labels("histogram", labels.clone()),
                Measurement::Histogram(StreamingIntegers::new()),
            ),
            (
                Key::from_name_and_labels("proxy.counter", labels),
                Measurement::Counter(13),
            ),
        ];
        expected.sort_by_key(|(k, _)| k.name());

        assert_eq!(snapshot.len(), expected.len());
        for rhs in expected {
            let lhs = snapshot.remove(0);
            assert_eq!(lhs.0, rhs.0);
            assert_eq!(mem::discriminant(&lhs.1), mem::discriminant(&rhs.1));
        }
    }
}
