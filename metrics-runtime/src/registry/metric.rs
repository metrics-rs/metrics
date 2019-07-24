use crate::common::{Identifier, Kind, Measurement, ValueHandle, ValueSnapshot};
use crate::config::Configuration;
use crate::data::Snapshot;
use crate::registry::ScopeRegistry;
use arc_swap::{ptr_eq, ArcSwap};
use im::hashmap::HashMap;
use metrics_core::Observer;
use quanta::Clock;
use std::ops::Deref;
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
            match self.metrics.lease().deref().get(&id) {
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

                    let metrics_ptr = self.metrics.lease();
                    let mut metrics = metrics_ptr.deref().clone();
                    match metrics.insert(id.clone(), value_handle.clone()) {
                        // Somebody else beat us to it, loop.
                        Some(_) => continue,
                        None => {
                            // If we weren't able to cleanly update the map, then try again.
                            let old = self
                                .metrics
                                .compare_and_swap(&metrics_ptr, Arc::new(metrics));
                            if !ptr_eq(old, metrics_ptr) {
                                continue;
                            }
                        }
                    }

                    return value_handle;
                }
            }
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut values = Vec::new();

        let metrics = self.metrics.load().deref().clone();
        for (id, value) in metrics.into_iter() {
            let (key, scope_handle, _) = id.into_parts();
            let scope = self.scope_registry.get(scope_handle);

            match value.snapshot() {
                ValueSnapshot::Single(measurement) => {
                    let key = key.map_name(|name| scope.into_string(name));
                    values.push((key, measurement));
                }
                ValueSnapshot::Multiple(mut measurements) => {
                    for (subkey, measurement) in measurements.drain(..) {
                        let scope = scope.clone();
                        let subkey = subkey.map_name(|name| scope.into_string(name));
                        values.push((subkey, measurement));
                    }
                }
            }
        }

        Snapshot::new(values)
    }

    pub fn observe<O: Observer>(&self, observer: &mut O) {
        let metrics = self.metrics.load().deref().clone();
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
