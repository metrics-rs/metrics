use crate::{
    data::Snapshot,
    registry::{MetricRegistry, ScopeRegistry},
};

use metrics_core::{Observe, Observer};

use std::sync::Arc;

/// Handle for acquiring snapshots.
///
/// `Controller` is [`metrics-core`]-compatible as a snapshot provider, both for synchronous and
/// asynchronous snapshotting.
///
/// [`metrics-core`]: https://docs.rs/metrics-core
#[derive(Clone)]
pub struct Controller {
    metric_registry: Arc<MetricRegistry>,
    scope_registry: Arc<ScopeRegistry>,
}

impl Controller {
    pub(crate) fn new(
        metric_registry: Arc<MetricRegistry>,
        scope_registry: Arc<ScopeRegistry>,
    ) -> Controller {
        Controller {
            metric_registry,
            scope_registry,
        }
    }

    /// Provide a snapshot of its collected metrics.
    pub fn snapshot(&self) -> Snapshot {
        self.metric_registry.snapshot()
    }
}

impl Observe for Controller {
    fn observe<O: Observer>(&self, observer: &mut O) {
        self.metric_registry.observe(observer)
    }
}
