use crate::common::{MetricIdentifier, MetricKind, MetricValue};
use crate::config::Configuration;
use crate::data::Snapshot;
use crate::registry::ScopeRegistry;
use arc_swap::{ptr_eq, ArcSwap};
use im::hashmap::HashMap;
use quanta::Clock;
use std::ops::Deref;
use std::sync::Arc;

pub(crate) struct MetricRegistry {
    scope_registry: Arc<ScopeRegistry>,
    metrics: ArcSwap<HashMap<MetricIdentifier, MetricValue>>,
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

    pub fn get_value_handle(&self, identifier: MetricIdentifier) -> MetricValue {
        loop {
            match self.metrics.lease().deref().get(&identifier) {
                Some(handle) => return handle.clone(),
                None => {
                    let kind = match &identifier {
                        MetricIdentifier::Unlabeled(_, _, kind) => kind,
                    };

                    let value_handle = match kind {
                        MetricKind::Counter => MetricValue::counter(),
                        MetricKind::Gauge => MetricValue::gauge(),
                        MetricKind::Histogram => MetricValue::histogram(
                            self.config.histogram_window,
                            self.config.histogram_granularity,
                            self.clock.clone(),
                        ),
                    };

                    let metrics_ptr = self.metrics.lease();
                    let mut metrics = metrics_ptr.deref().clone();
                    match metrics.insert(identifier.clone(), value_handle.clone()) {
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
        for (identifier, value) in metrics.into_iter() {
            let (name, scope_handle) = match identifier {
                MetricIdentifier::Unlabeled(name, scope, _) => (name, scope),
            };

            let scope = self.scope_registry.get(scope_handle);
            let scoped_name = scope.into_scoped(name);
            let snapshot = value.snapshot();
            named_values.push((scoped_name, snapshot));
        }

        Snapshot::from(named_values)
    }
}
