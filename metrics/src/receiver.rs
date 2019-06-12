use crate::{
    builder::{Builder, BuilderError},
    common::MetricScope,
    config::Configuration,
    control::Controller,
    registry::{MetricRegistry, ScopeRegistry},
    sink::Sink,
};
use metrics_core::Key;
use metrics_facade::Recorder;
use quanta::{Builder as UpkeepBuilder, Clock, Handle as UpkeepHandle};
use std::cell::RefCell;
use std::sync::Arc;

thread_local! {
    static SINK: RefCell<Option<Sink>> = RefCell::new(None);
}

/// Central store for metrics.
///
/// `Receiver` is the nucleus for all metrics operations.  While no operations are performed by it
/// directly, it holds the registeries and references to resources and so it must live as long as
/// any [`Sink`] or [`Controller`] does.
pub struct Receiver {
    metric_registry: Arc<MetricRegistry>,
    scope_registry: Arc<ScopeRegistry>,
    clock: Clock,
    _upkeep_handle: UpkeepHandle,
}

impl Receiver {
    pub(crate) fn from_config(config: Configuration) -> Result<Receiver, BuilderError> {
        // Configure our clock and configure the quanta upkeep thread. The upkeep thread does that
        // for us, and keeps us within `upkeep_interval` of the true time.  The reads of this cache
        // time are faster than calling the underlying time source directly, and for histogram
        // windowing, we can afford to have a very granular value compared to the raw nanosecond
        // precsion provided by quanta by default.
        let clock = Clock::new();
        let upkeep = UpkeepBuilder::new_with_clock(config.upkeep_interval, clock.clone());
        let _upkeep_handle = upkeep.start().map_err(|_| BuilderError::UpkeepFailure)?;

        let scope_registry = Arc::new(ScopeRegistry::new());
        let metric_registry = Arc::new(MetricRegistry::new(
            scope_registry.clone(),
            config,
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

    /// Installs this receiver as the global metrics facade.
    pub fn install(self) {
        metrics_facade::set_boxed_recorder(Box::new(self)).unwrap();
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

impl Recorder for Receiver {
    fn record_counter(&self, key: Key, value: u64) {
        SINK.with(move |sink| {
            let mut sink = sink.borrow_mut();
            if sink.is_none() {
                let new_sink = self.get_sink();
                *sink = Some(new_sink);
            }

            sink.as_mut().unwrap().record_count(key, value);
        });
    }

    fn record_gauge(&self, key: Key, value: i64) {
        SINK.with(move |sink| {
            let mut sink = sink.borrow_mut();
            if sink.is_none() {
                let new_sink = self.get_sink();
                *sink = Some(new_sink);
            }

            sink.as_mut().unwrap().record_gauge(key, value);
        });
    }

    fn record_histogram(&self, key: Key, value: u64) {
        SINK.with(move |sink| {
            let mut sink = sink.borrow_mut();
            if sink.is_none() {
                let new_sink = self.get_sink();
                *sink = Some(new_sink);
            }

            sink.as_mut().unwrap().record_value(key, value);
        });
    }
}
