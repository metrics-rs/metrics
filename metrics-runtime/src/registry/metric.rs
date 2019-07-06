use crate::common::{Identifier, Kind, ValueHandle};
use crate::config::Configuration;
use crate::data::Snapshot;
use crate::registry::ScopeRegistry;
use arc_swap::{ptr_eq, ArcSwap};
use im::hashmap::HashMap;
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

    pub fn get_snapshot(&self) -> Snapshot {
        let mut named_values = Vec::new();

        let metrics = self.metrics.load().deref().clone();
        for (id, value) in metrics.into_iter() {
            let (key, scope_handle, _) = id.into_parts();
            let scope = self.scope_registry.get(scope_handle);
            let key = key.map_name(|name| scope.into_scoped(name));

            let snapshot = value.snapshot();
            named_values.push((key, snapshot));
        }

        Snapshot::new(named_values)
    }
}
