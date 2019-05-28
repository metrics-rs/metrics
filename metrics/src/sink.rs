use crate::{
    common::{
        Delta, MetricIdentifier, MetricKind, MetricName, MetricScope, MetricScopeHandle,
        MetricValue,
    },
    data::{Counter, Gauge, Histogram},
    registry::{MetricRegistry, ScopeRegistry},
};
use fxhash::FxBuildHasher;
use quanta::Clock;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

type FastHashMap<K, V> = hashbrown::HashMap<K, V, FxBuildHasher>;

/// Errors during sink creation.
#[derive(Debug, Clone)]
pub enum SinkError {
    /// The scope value given was invalid i.e. empty or illegal characters.
    InvalidScope,
}

impl Error for SinkError {}

impl fmt::Display for SinkError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SinkError::InvalidScope => write!(f, "given scope is invalid"),
        }
    }
}

/// A value that can be used as a metric scope.
///
/// This helper trait allows us to accept either a single string or a slice of strings to use as a
/// scope, to avoid needing to allocate in the case where we want to be able to specify multiple
/// scope levels in a single go.
pub trait AsScoped<'a> {
    fn as_scoped(&'a self, base: MetricScope) -> MetricScope;
}

/// Handle for sending metric samples.
pub struct Sink {
    metric_registry: Arc<MetricRegistry>,
    metric_cache: FastHashMap<MetricIdentifier, MetricValue>,
    scope_registry: Arc<ScopeRegistry>,
    scope: MetricScope,
    scope_handle: MetricScopeHandle,
    clock: Clock,
}

impl Sink {
    pub(crate) fn new(
        metric_registry: Arc<MetricRegistry>,
        scope_registry: Arc<ScopeRegistry>,
        scope: MetricScope,
        clock: Clock,
    ) -> Sink {
        let scope_handle = scope_registry.register(scope.clone());

        Sink {
            metric_registry,
            metric_cache: FastHashMap::default(),
            scope_registry,
            scope,
            scope_handle,
            clock,
        }
    }

    /// Creates a scoped clone of this [`Sink`].
    ///
    /// Scoping controls the resulting metric name for any metrics sent by this [`Sink`].  For
    /// example, you might have a metric called `messages_sent`.
    ///
    /// With scoping, you could have independent versions of the same metric.  This is useful for
    /// having the same "base" metric name but with broken down values.
    ///
    /// Going further with the above example, if you had a server, and listened on multiple
    /// addresses, maybe you would have a scoped [`Sink`] per listener, and could end up with
    /// metrics that look like this:
    /// - `listener.a.messages_sent`
    /// - `listener.b.messages_sent`
    /// - `listener.c.messages_sent`
    /// - etc
    ///
    /// Scopes are also inherited.  If you create a scoped [`Sink`] from another [`Sink`] which is
    /// already scoped, the scopes will be merged together using a `.` as the string separator.
    /// This makes it easy to nest scopes.  Cloning a scoped [`Sink`], though, will inherit the
    /// same scope as the original.
    pub fn scoped<'a, S: AsScoped<'a> + ?Sized>(&self, scope: &'a S) -> Sink {
        let new_scope = scope.as_scoped(self.scope.clone());

        Sink::new(
            self.metric_registry.clone(),
            self.scope_registry.clone(),
            new_scope,
            self.clock.clone(),
        )
    }

    /// Gets the current time, in nanoseconds, from the internal high-speed clock.
    pub fn now(&self) -> u64 {
        self.clock.now()
    }

    /// Records a value for a counter identified by the given name.
    pub fn record_count<N: Into<MetricName>>(&mut self, name: N, value: u64) {
        let identifier =
            MetricIdentifier::Unlabeled(name.into(), self.scope_handle, MetricKind::Counter);
        let value_handle = self.get_cached_value_handle(identifier);
        value_handle.update_counter(value);
    }

    /// Records the value for a gauge identified by the given name.
    pub fn record_gauge<N: Into<MetricName>>(&mut self, name: N, value: i64) {
        let identifier =
            MetricIdentifier::Unlabeled(name.into(), self.scope_handle, MetricKind::Gauge);
        let value_handle = self.get_cached_value_handle(identifier);
        value_handle.update_gauge(value);
    }

    /// Records the value for a timing histogram identified by the given name.
    ///
    /// Both the start and end times must be supplied, but any values that implement [`Delta`] can
    /// be used which allows for raw values from [`quanta::Clock`] to be used, or measurements from
    /// [`Instant::now`].
    pub fn record_timing<N: Into<MetricName>, V: Delta>(&mut self, name: N, start: V, end: V) {
        let value = end.delta(start);
        self.record_value(name, value);
    }

    /// Records the value for a value histogram identified by the given name.
    pub fn record_value<N: Into<MetricName>>(&mut self, name: N, value: u64) {
        let identifier =
            MetricIdentifier::Unlabeled(name.into(), self.scope_handle, MetricKind::Histogram);
        let value_handle = self.get_cached_value_handle(identifier);
        value_handle.update_histogram(value);
    }

    /// Creates a handle to the given counter.
    ///
    /// This handle can be embedded into an existing type and used to directly update the
    /// underlying counter.  It is merely a proxy, so multiple handles to the same counter can be
    /// held and used.
    pub fn counter<N: Into<MetricName>>(&mut self, name: N) -> Counter {
        let identifier =
            MetricIdentifier::Unlabeled(name.into(), self.scope_handle, MetricKind::Counter);
        self.get_cached_value_handle(identifier).clone().into()
    }

    /// Creates a handle to the given gauge.
    ///
    /// This handle can be embedded into an existing type and used to directly update the
    /// underlying gauge.  It is merely a proxy, so multiple handles to the same gauge can be
    /// held and used.
    pub fn gauge<N: Into<MetricName>>(&mut self, name: N) -> Gauge {
        let identifier =
            MetricIdentifier::Unlabeled(name.into(), self.scope_handle, MetricKind::Gauge);
        self.get_cached_value_handle(identifier).clone().into()
    }

    /// Creates a handle to the given histogram.
    ///
    /// This handle can be embedded into an existing type and used to directly update the
    /// underlying histogram.  It is merely a proxy, so multiple handles to the same histogram
    /// can be held and used.
    pub fn histogram<N: Into<MetricName>>(&mut self, name: N) -> Histogram {
        let identifier =
            MetricIdentifier::Unlabeled(name.into(), self.scope_handle, MetricKind::Histogram);
        self.get_cached_value_handle(identifier).clone().into()
    }

    fn get_cached_value_handle(&mut self, identifier: MetricIdentifier) -> &MetricValue {
        // This gross hack gets around lifetime rules until full NLL is stable.  Without it, the
        // borrow checker doesn't understand the flow control and thinks the reference lives all
        // the way until the of the function, which breaks when we try to take a mutable reference
        // for inserting into the handle cache.
        if let Some(handle) = self.metric_cache.get(&identifier) {
            return unsafe { &*(handle as *const MetricValue) };
        }

        let handle = self.metric_registry.get_value_handle(identifier.clone());
        self.metric_cache.insert(identifier.clone(), handle);
        self.metric_cache.get(&identifier).unwrap()
    }
}

impl Clone for Sink {
    fn clone(&self) -> Sink {
        Sink {
            metric_registry: self.metric_registry.clone(),
            metric_cache: self.metric_cache.clone(),
            scope_registry: self.scope_registry.clone(),
            scope: self.scope.clone(),
            scope_handle: self.scope_handle,
            clock: self.clock.clone(),
        }
    }
}

impl<'a> AsScoped<'a> for str {
    fn as_scoped(&'a self, base: MetricScope) -> MetricScope {
        match base {
            MetricScope::Root => {
                let parts = vec![self.to_owned()];
                MetricScope::Nested(parts)
            }
            MetricScope::Nested(mut parts) => {
                parts.push(self.to_owned());
                MetricScope::Nested(parts)
            }
        }
    }
}

impl<'a, 'b, T> AsScoped<'a> for T
where
    &'a T: AsRef<[&'b str]>,
    T: 'a,
{
    fn as_scoped(&'a self, base: MetricScope) -> MetricScope {
        match base {
            MetricScope::Root => {
                let parts = self.as_ref().iter().map(|s| s.to_string()).collect();
                MetricScope::Nested(parts)
            }
            MetricScope::Nested(mut parts) => {
                let mut new_parts = self.as_ref().iter().map(|s| s.to_string()).collect();
                parts.append(&mut new_parts);
                MetricScope::Nested(parts)
            }
        }
    }
}
