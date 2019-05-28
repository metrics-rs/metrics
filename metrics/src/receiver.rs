use crate::{
    builder::{Builder, BuilderError},
    common::MetricScope,
    config::MetricConfiguration,
    control::Controller,
    registry::{MetricRegistry, ScopeRegistry},
    sink::Sink,
};
use quanta::{Builder as UpkeepBuilder, Clock, Handle as UpkeepHandle};
use std::sync::Arc;
use std::time::Duration;

/// Central store for metrics.
///
/// `Receiver` is the nucleus for all metrics operations.  While no operations are performed by it
/// directly, it holds the registeries and references to resources and so it must live as long as
/// any [`Sink`] or `[`Controller`] does.
pub struct Receiver {
    metric_registry: Arc<MetricRegistry>,
    scope_registry: Arc<ScopeRegistry>,
    clock: Clock,
    _upkeep_handle: UpkeepHandle,
}

impl Receiver {
    pub(crate) fn from_builder(builder: Builder) -> Result<Receiver, BuilderError> {
        // Configure our clock and configure the quanta upkeep thread. The upkeep thread does that
        // for us, and keeps us within `upkeep_interval` of the true time.  The reads of this cache
        // time are faster than calling the underlying time source directly, and for histogram
        // windowing, we can afford to have a very granular value compared to the raw nanosecond
        // precsion provided by quanta by default.
        let clock = Clock::new();
        let upkeep_interval = Duration::from_millis(50);
        let upkeep = UpkeepBuilder::new_with_clock(upkeep_interval, clock.clone());
        let _upkeep_handle = upkeep.start().map_err(|_| BuilderError::UpkeepFailure)?;

        let metric_config = MetricConfiguration::from_builder(&builder);

        let scope_registry = Arc::new(ScopeRegistry::new());
        let metric_registry = Arc::new(MetricRegistry::new(
            scope_registry.clone(),
            metric_config,
            clock.clone(),
        ));

        Ok(Receiver {
            metric_registry,
            scope_registry,
            clock,
            _upkeep_handle,
        })
    }

    /// Creates a new [`Builder`] for building a [`Receiver`].
    pub fn builder() -> Builder {
        Builder::default()
    }

    /// Creates a [`Sink`] bound to this receiver.
    pub fn get_sink(&self) -> Sink {
        Sink::new(
            self.metric_registry.clone(),
            self.scope_registry.clone(),
            MetricScope::Root,
            self.clock.clone(),
        )
    }

    /// Creates a [`Controller`] bound to this receiver.
    pub fn get_controller(&self) -> Controller {
        Controller::new(self.metric_registry.clone(), self.scope_registry.clone())
    }
}
