use crate::{
    common::{Delta, Identifier, Kind, Scope, ScopeHandle, ValueHandle},
    data::{Counter, Gauge, Histogram},
    registry::{MetricRegistry, ScopeRegistry},
};
use fxhash::FxBuildHasher;
use metrics_core::{IntoKey, IntoLabels, ScopedString};
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
    /// Creates a new [`Scope`] by adding `self` to the `base` scope.
    fn as_scoped(&'a self, base: Scope) -> Scope;
}

/// Handle for sending metric samples.
pub struct Sink {
    metric_registry: Arc<MetricRegistry>,
    metric_cache: FastHashMap<Identifier, ValueHandle>,
    scope_registry: Arc<ScopeRegistry>,
    scope: Scope,
    scope_handle: ScopeHandle,
    clock: Clock,
}

impl Sink {
    pub(crate) fn new(
        metric_registry: Arc<MetricRegistry>,
        scope_registry: Arc<ScopeRegistry>,
        scope: Scope,
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
    pub fn record_counter<N>(&mut self, name: N, value: u64)
    where
        N: IntoKey,
    {
        let id = Identifier::new(name.into_key(), self.scope_handle, Kind::Counter);
        let value_handle = self.get_cached_value_handle(id);
        value_handle.update_counter(value);
    }

    /// Records a value for a counter identified by the given name and labels.
    pub fn record_counter_with_labels<N, L>(&mut self, name: N, labels: L, value: u64)
    where
        N: Into<ScopedString>,
        L: IntoLabels,
    {
        let id = Identifier::new((name, labels), self.scope_handle, Kind::Counter);
        let value_handle = self.get_cached_value_handle(id);
        value_handle.update_counter(value);
    }

    /// Records a value for a gauge identified by the given name.
    pub fn record_gauge<N>(&mut self, name: N, value: i64)
    where
        N: IntoKey,
    {
        let id = Identifier::new(name.into_key(), self.scope_handle, Kind::Gauge);
        let value_handle = self.get_cached_value_handle(id);
        value_handle.update_gauge(value);
    }

    /// Records a value for a gauge identified by the given name and labels.
    pub fn record_gauge_with_labels<N, L>(&mut self, name: N, labels: L, value: i64)
    where
        N: Into<ScopedString>,
        L: IntoLabels,
    {
        let id = Identifier::new((name, labels), self.scope_handle, Kind::Gauge);
        let value_handle = self.get_cached_value_handle(id);
        value_handle.update_gauge(value);
    }

    /// Records the value for a timing histogram identified by the given name.
    ///
    /// Both the start and end times must be supplied, but any values that implement [`Delta`] can
    /// be used which allows for raw values from [`quanta::Clock`] to be used, or measurements from
    /// [`Instant::now`].
    pub fn record_timing<N, V>(&mut self, name: N, start: V, end: V)
    where
        N: IntoKey,
        V: Delta,
    {
        let delta = end.delta(start);
        self.record_value(name, delta);
    }

    /// Records the value for a timing histogram identified by the given name and labels.
    ///
    /// Both the start and end times must be supplied, but any values that implement [`Delta`] can
    /// be used which allows for raw values from [`quanta::Clock`] to be used, or measurements from
    /// [`Instant::now`].
    pub fn record_timing_with_labels<N, L, V>(&mut self, name: N, labels: L, start: V, end: V)
    where
        N: Into<ScopedString>,
        L: IntoLabels,
        V: Delta,
    {
        let delta = end.delta(start);
        self.record_value_with_labels(name, labels, delta);
    }

    /// Records the value for a value histogram identified by the given name.
    pub fn record_value<N>(&mut self, name: N, value: u64)
    where
        N: IntoKey,
    {
        let id = Identifier::new(name.into_key(), self.scope_handle, Kind::Histogram);
        let value_handle = self.get_cached_value_handle(id);
        value_handle.update_histogram(value);
    }

    /// Records the value for a value histogram identified by the given name and labels.
    pub fn record_value_with_labels<N, L>(&mut self, name: N, labels: L, value: u64)
    where
        N: Into<ScopedString>,
        L: IntoLabels,
    {
        let id = Identifier::new((name, labels), self.scope_handle, Kind::Histogram);
        let value_handle = self.get_cached_value_handle(id);
        value_handle.update_histogram(value);
    }

    /// Creates a handle to the given counter.
    ///
    /// This handle can be embedded into an existing type and used to directly update the
    /// underlying counter.  It is merely a proxy, so multiple handles to the same counter can be
    /// held and used.
    pub fn counter<N>(&mut self, name: N) -> Counter
    where
        N: IntoKey,
    {
        self.get_owned_value_handle(name, Kind::Counter).into()
    }

    /// Creates a handle to the given counter, with labels attached.
    ///
    /// This handle can be embedded into an existing type and used to directly update the
    /// underlying counter.  It is merely a proxy, so multiple handles to the same counter can be
    /// held and used.
    pub fn counter_with_labels<N, L>(&mut self, name: N, labels: L) -> Counter
    where
        N: Into<ScopedString>,
        L: IntoLabels,
    {
        self.get_owned_value_handle((name, labels), Kind::Counter)
            .into()
    }

    /// Creates a handle to the given gauge.
    ///
    /// This handle can be embedded into an existing type and used to directly update the
    /// underlying gauge.  It is merely a proxy, so multiple handles to the same gauge can be
    /// held and used.
    pub fn gauge<N>(&mut self, name: N) -> Gauge
    where
        N: IntoKey,
    {
        self.get_owned_value_handle(name, Kind::Gauge).into()
    }

    /// Creates a handle to the given gauge.
    ///
    /// This handle can be embedded into an existing type and used to directly update the
    /// underlying gauge.  It is merely a proxy, so multiple handles to the same gauge can be
    /// held and used.
    pub fn gauge_with_labels<N, L>(&mut self, name: N, labels: L) -> Gauge
    where
        N: Into<ScopedString>,
        L: IntoLabels,
    {
        self.get_owned_value_handle((name, labels), Kind::Gauge)
            .into()
    }

    /// Creates a handle to the given histogram.
    ///
    /// This handle can be embedded into an existing type and used to directly update the
    /// underlying histogram.  It is merely a proxy, so multiple handles to the same histogram
    /// can be held and used.
    pub fn histogram<N>(&mut self, name: N) -> Histogram
    where
        N: IntoKey,
    {
        self.get_owned_value_handle(name, Kind::Histogram).into()
    }

    /// Creates a handle to the given histogram.
    ///
    /// This handle can be embedded into an existing type and used to directly update the
    /// underlying histogram.  It is merely a proxy, so multiple handles to the same histogram
    /// can be held and used.
    pub fn histogram_with_labels<N, L>(&mut self, name: N, labels: L) -> Histogram
    where
        N: Into<ScopedString>,
        L: IntoLabels,
    {
        self.get_owned_value_handle((name, labels), Kind::Histogram)
            .into()
    }

    fn get_owned_value_handle<K>(&mut self, key: K, kind: Kind) -> ValueHandle
    where
        K: IntoKey,
    {
        let id = Identifier::new(key.into_key(), self.scope_handle, kind);
        self.get_cached_value_handle(id).clone().into()
    }

    fn get_cached_value_handle(&mut self, identifier: Identifier) -> &ValueHandle {
        // This gross hack gets around lifetime rules until full NLL is stable.  Without it, the
        // borrow checker doesn't understand the flow control and thinks the reference lives all
        // the way until the of the function, which breaks when we try to take a mutable reference
        // for inserting into the handle cache.
        if let Some(handle) = self.metric_cache.get(&identifier) {
            return unsafe { &*(handle as *const ValueHandle) };
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
    fn as_scoped(&'a self, base: Scope) -> Scope {
        match base {
            Scope::Root => {
                let parts = vec![self.to_owned()];
                Scope::Nested(parts)
            }
            Scope::Nested(mut parts) => {
                parts.push(self.to_owned());
                Scope::Nested(parts)
            }
        }
    }
}

impl<'a, 'b, T> AsScoped<'a> for T
where
    &'a T: AsRef<[&'b str]>,
    T: 'a,
{
    fn as_scoped(&'a self, base: Scope) -> Scope {
        match base {
            Scope::Root => {
                let parts = self.as_ref().iter().map(|s| s.to_string()).collect();
                Scope::Nested(parts)
            }
            Scope::Nested(mut parts) => {
                let mut new_parts = self.as_ref().iter().map(|s| s.to_string()).collect();
                parts.append(&mut new_parts);
                Scope::Nested(parts)
            }
        }
    }
}
