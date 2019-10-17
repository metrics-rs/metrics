use crate::{
    common::{Delta, Identifier, Kind, Measurement, Scope, ScopeHandle, ValueHandle},
    data::{Counter, Gauge, Histogram},
    registry::{MetricRegistry, ScopeRegistry},
};
use hashbrown::HashMap;
use metrics_core::{IntoLabels, Key, Label, ScopedString};
use quanta::Clock;
use std::{error::Error, fmt, sync::Arc};

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
#[derive(Debug)]
pub struct Sink {
    metric_registry: Arc<MetricRegistry>,
    metric_cache: HashMap<Identifier, ValueHandle>,
    scope_registry: Arc<ScopeRegistry>,
    scope: Scope,
    scope_handle: ScopeHandle,
    clock: Clock,
    default_labels: Vec<Label>,
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
            metric_cache: HashMap::default(),
            scope_registry,
            scope,
            scope_handle,
            clock,
            default_labels: Vec::new(),
        }
    }

    /// Adds default labels for this sink and any derived sinks.
    ///
    /// Default labels are added to all metrics.  If a metric is updated and requested and it has
    /// its own labels specified, the default labels will be appended to the existing labels.
    ///
    /// Labels are passed on, with scope, to any scoped children or cloned sinks.
    pub fn add_default_labels<L>(&mut self, labels: L)
    where
        L: IntoLabels,
    {
        let labels = labels.into_labels();
        self.default_labels.extend(labels);
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

        let mut sink = Sink::new(
            self.metric_registry.clone(),
            self.scope_registry.clone(),
            new_scope,
            self.clock.clone(),
        );
        if !self.default_labels.is_empty() {
            sink.add_default_labels(self.default_labels.clone());
        }

        sink
    }

    /// Gets the current time, in nanoseconds, from the internal high-speed clock.
    pub fn now(&self) -> u64 {
        self.clock.now()
    }

    /// Increment a value for a counter identified by the given name.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate metrics_runtime;
    /// # use metrics_runtime::Receiver;
    /// # fn main() {
    /// let receiver = Receiver::builder().build().expect("failed to create receiver");
    /// let mut sink = receiver.sink();
    /// sink.increment_counter("messages_processed", 1);
    /// # }
    /// ```
    pub fn increment_counter<N>(&mut self, name: N, value: u64)
    where
        N: Into<Key>,
    {
        let key = self.construct_key(name);
        let id = Identifier::new(key, self.scope_handle, Kind::Counter);
        let value_handle = self.get_cached_value_handle(id);
        value_handle.update_counter(value);
    }

    /// Increment a value for a counter identified by the given name and labels.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate metrics_runtime;
    /// # use metrics_runtime::Receiver;
    /// # fn main() {
    /// let receiver = Receiver::builder().build().expect("failed to create receiver");
    /// let mut sink = receiver.sink();
    /// sink.increment_counter_with_labels("messages_processed", 1, &[("message_type", "mgmt")]);
    /// # }
    /// ```
    pub fn increment_counter_with_labels<N, L>(&mut self, name: N, value: u64, labels: L)
    where
        N: Into<ScopedString>,
        L: IntoLabels,
    {
        let key = self.construct_key((name, labels));
        let id = Identifier::new(key, self.scope_handle, Kind::Counter);
        let value_handle = self.get_cached_value_handle(id);
        value_handle.update_counter(value);
    }

    /// Update a value for a gauge identified by the given name.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate metrics_runtime;
    /// # use metrics_runtime::Receiver;
    /// # fn main() {
    /// let receiver = Receiver::builder().build().expect("failed to create receiver");
    /// let mut sink = receiver.sink();
    /// sink.update_gauge("current_offset", -131);
    /// # }
    /// ```
    pub fn update_gauge<N>(&mut self, name: N, value: i64)
    where
        N: Into<Key>,
    {
        let key = self.construct_key(name);
        let id = Identifier::new(key, self.scope_handle, Kind::Gauge);
        let value_handle = self.get_cached_value_handle(id);
        value_handle.update_gauge(value);
    }

    /// Update a value for a gauge identified by the given name and labels.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate metrics_runtime;
    /// # use metrics_runtime::Receiver;
    /// # fn main() {
    /// let receiver = Receiver::builder().build().expect("failed to create receiver");
    /// let mut sink = receiver.sink();
    /// sink.update_gauge_with_labels("current_offset", -131, &[("source", "stratum-1")]);
    /// # }
    /// ```
    pub fn update_gauge_with_labels<N, L>(&mut self, name: N, value: i64, labels: L)
    where
        N: Into<ScopedString>,
        L: IntoLabels,
    {
        let key = self.construct_key((name, labels));
        let id = Identifier::new(key, self.scope_handle, Kind::Gauge);
        let value_handle = self.get_cached_value_handle(id);
        value_handle.update_gauge(value);
    }

    /// Records the value for a timing histogram identified by the given name.
    ///
    /// Both the start and end times must be supplied, but any values that implement [`Delta`] can
    /// be used which allows for raw values from [`quanta::Clock`] to be used, or measurements from
    /// [`Instant::now`](std::time::Instant::now).
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate metrics_runtime;
    /// # use metrics_runtime::Receiver;
    /// # use std::thread;
    /// # use std::time::Duration;
    /// # fn main() {
    /// let receiver = Receiver::builder().build().expect("failed to create receiver");
    /// let mut sink = receiver.sink();
    /// let start = sink.now();
    /// thread::sleep(Duration::from_millis(10));
    /// let end = sink.now();
    /// sink.record_timing("sleep_time", start, end);
    /// # }
    /// ```
    pub fn record_timing<N, V>(&mut self, name: N, start: V, end: V)
    where
        N: Into<Key>,
        V: Delta,
    {
        let delta = end.delta(start);
        self.record_value(name, delta);
    }

    /// Records the value for a timing histogram identified by the given name and labels.
    ///
    /// Both the start and end times must be supplied, but any values that implement [`Delta`] can
    /// be used which allows for raw values from [`quanta::Clock`] to be used, or measurements from
    /// [`Instant::now`](std::time::Instant::now).
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate metrics_runtime;
    /// # use metrics_runtime::Receiver;
    /// # use std::thread;
    /// # use std::time::Duration;
    /// # fn main() {
    /// let receiver = Receiver::builder().build().expect("failed to create receiver");
    /// let mut sink = receiver.sink();
    /// let start = sink.now();
    /// thread::sleep(Duration::from_millis(10));
    /// let end = sink.now();
    /// sink.record_timing_with_labels("sleep_time", start, end, &[("mode", "low_priority")]);
    /// # }
    /// ```
    pub fn record_timing_with_labels<N, L, V>(&mut self, name: N, start: V, end: V, labels: L)
    where
        N: Into<ScopedString>,
        L: IntoLabels,
        V: Delta,
    {
        let delta = end.delta(start);
        self.record_value_with_labels(name, delta, labels);
    }

    /// Records the value for a value histogram identified by the given name.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate metrics_runtime;
    /// # use metrics_runtime::Receiver;
    /// # use std::thread;
    /// # use std::time::Duration;
    /// # fn main() {
    /// let receiver = Receiver::builder().build().expect("failed to create receiver");
    /// let mut sink = receiver.sink();
    /// sink.record_value("rows_returned", 42);
    /// # }
    /// ```
    pub fn record_value<N>(&mut self, name: N, value: u64)
    where
        N: Into<Key>,
    {
        let key = self.construct_key(name);
        let id = Identifier::new(key, self.scope_handle, Kind::Histogram);
        let value_handle = self.get_cached_value_handle(id);
        value_handle.update_histogram(value);
    }

    /// Records the value for a value histogram identified by the given name and labels.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate metrics_runtime;
    /// # use metrics_runtime::Receiver;
    /// # use std::thread;
    /// # use std::time::Duration;
    /// # fn main() {
    /// let receiver = Receiver::builder().build().expect("failed to create receiver");
    /// let mut sink = receiver.sink();
    /// sink.record_value_with_labels("rows_returned", 42, &[("table", "posts")]);
    /// # }
    /// ```
    pub fn record_value_with_labels<N, L>(&mut self, name: N, value: u64, labels: L)
    where
        N: Into<ScopedString>,
        L: IntoLabels,
    {
        let key = self.construct_key((name, labels));
        let id = Identifier::new(key, self.scope_handle, Kind::Histogram);
        let value_handle = self.get_cached_value_handle(id);
        value_handle.update_histogram(value);
    }

    /// Creates a handle to the given counter.
    ///
    /// This handle can be embedded into an existing type and used to directly update the
    /// underlying counter without requiring a [`Sink`].  This method can be called multiple times
    /// with the same `name` and the handle will point to the single underlying instance.
    ///
    /// [`Counter`] is clonable.
    ///`
    /// # Examples
    ///
    /// ```rust
    /// # extern crate metrics_runtime;
    /// # use metrics_runtime::Receiver;
    /// # fn main() {
    /// let receiver = Receiver::builder().build().expect("failed to create receiver");
    /// let mut sink = receiver.sink();
    /// let counter = sink.counter("messages_processed");
    /// counter.record(1);
    ///
    /// // Alternate, simpler usage:
    /// counter.increment();
    /// # }
    /// ```
    pub fn counter<N>(&mut self, name: N) -> Counter
    where
        N: Into<Key>,
    {
        let key = self.construct_key(name);
        self.get_owned_value_handle(key, Kind::Counter).into()
    }

    /// Creates a handle to the given counter, with labels attached.
    ///
    /// This handle can be embedded into an existing type and used to directly update the
    /// underlying counter without requiring a [`Sink`].  This method can be called multiple times
    /// with the same `name`/`labels` and the handle will point to the single underlying instance.
    ///
    /// [`Counter`] is clonable.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate metrics_runtime;
    /// # use metrics_runtime::Receiver;
    /// # fn main() {
    /// let receiver = Receiver::builder().build().expect("failed to create receiver");
    /// let mut sink = receiver.sink();
    /// let counter = sink.counter_with_labels("messages_processed", &[("service", "secure")]);
    /// counter.record(1);
    ///
    /// // Alternate, simpler usage:
    /// counter.increment();
    /// # }
    /// ```
    pub fn counter_with_labels<N, L>(&mut self, name: N, labels: L) -> Counter
    where
        N: Into<ScopedString>,
        L: IntoLabels,
    {
        self.counter((name, labels))
    }

    /// Creates a handle to the given gauge.
    ///
    /// This handle can be embedded into an existing type and used to directly update the
    /// underlying gauge without requiring a [`Sink`].  This method can be called multiple times
    /// with the same `name` and the handle will point to the single underlying instance.
    ///
    /// [`Gauge`] is clonable.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate metrics_runtime;
    /// # use metrics_runtime::Receiver;
    /// # fn main() {
    /// let receiver = Receiver::builder().build().expect("failed to create receiver");
    /// let mut sink = receiver.sink();
    /// let gauge = sink.gauge("current_offset");
    /// gauge.record(-131);
    /// # }
    /// ```
    pub fn gauge<N>(&mut self, name: N) -> Gauge
    where
        N: Into<Key>,
    {
        let key = self.construct_key(name);
        self.get_owned_value_handle(key, Kind::Gauge).into()
    }

    /// Creates a handle to the given gauge, with labels attached.
    ///
    /// This handle can be embedded into an existing type and used to directly update the
    /// underlying gauge without requiring a [`Sink`].  This method can be called multiple times
    /// with the same `name`/`labels` and the handle will point to the single underlying instance.
    ///
    /// [`Gauge`] is clonable.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate metrics_runtime;
    /// # use metrics_runtime::Receiver;
    /// # fn main() {
    /// let receiver = Receiver::builder().build().expect("failed to create receiver");
    /// let mut sink = receiver.sink();
    /// let gauge = sink.gauge_with_labels("current_offset", &[("source", "stratum-1")]);
    /// gauge.record(-131);
    /// # }
    /// ```
    pub fn gauge_with_labels<N, L>(&mut self, name: N, labels: L) -> Gauge
    where
        N: Into<ScopedString>,
        L: IntoLabels,
    {
        self.gauge((name, labels))
    }

    /// Creates a handle to the given histogram.
    ///
    /// This handle can be embedded into an existing type and used to directly update the
    /// underlying histogram without requiring a [`Sink`].  This method can be called multiple
    /// times with the same `name` and the handle will point to the single underlying instance.
    ///
    /// [`Histogram`] is clonable.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate metrics_runtime;
    /// # use metrics_runtime::Receiver;
    /// # use std::thread;
    /// # use std::time::Duration;
    /// # fn main() {
    /// let receiver = Receiver::builder().build().expect("failed to create receiver");
    /// let mut sink = receiver.sink();
    /// let histogram = sink.histogram("request_duration");
    ///
    /// let start = sink.now();
    /// thread::sleep(Duration::from_millis(10));
    /// let end = sink.now();
    /// histogram.record_timing(start, end);
    ///
    /// // Alternatively, you can just push the raw value into a histogram:
    /// let delta = end - start;
    /// histogram.record_value(delta);
    /// # }
    /// ```
    pub fn histogram<N>(&mut self, name: N) -> Histogram
    where
        N: Into<Key>,
    {
        let key = self.construct_key(name);
        self.get_owned_value_handle(key, Kind::Histogram).into()
    }

    /// Creates a handle to the given histogram, with labels attached.
    ///
    /// This handle can be embedded into an existing type and used to directly update the
    /// underlying histogram without requiring a [`Sink`].  This method can be called multiple
    /// times with the same `name` and the handle will point to the single underlying instance.
    ///
    /// [`Histogram`] is clonable.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate metrics_runtime;
    /// # use metrics_runtime::Receiver;
    /// # use std::thread;
    /// # use std::time::Duration;
    /// # fn main() {
    /// let receiver = Receiver::builder().build().expect("failed to create receiver");
    /// let mut sink = receiver.sink();
    /// let histogram = sink.histogram_with_labels("request_duration", &[("service", "secure")]);
    ///
    /// let start = sink.now();
    /// thread::sleep(Duration::from_millis(10));
    /// let end = sink.now();
    /// histogram.record_timing(start, end);
    ///
    /// // Alternatively, you can just push the raw value into a histogram:
    /// let delta = end - start;
    /// histogram.record_value(delta);
    /// # }
    /// ```
    pub fn histogram_with_labels<N, L>(&mut self, name: N, labels: L) -> Histogram
    where
        N: Into<ScopedString>,
        L: IntoLabels,
    {
        self.histogram((name, labels))
    }

    /// Creates a proxy metric.
    ///
    /// Proxy metrics allow you to register a closure that, when a snapshot of the metric state is
    /// requested, will be called and have a chance to return multiple metrics that are added to
    /// the overall metric of actual metrics.
    ///
    /// This can be useful for metrics which are expensive to constantly recalculate/poll, allowing
    /// you to avoid needing to calculate/push them them yourself, with all the boilerplate that
    /// comes with doing so periodically.
    ///
    /// Individual metrics must provide their own key (name), which will be appended to the name
    /// given when registering the proxy.  A proxy can be reregistered at any time by calling this
    /// function again with the same name.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate metrics_runtime;
    /// # extern crate metrics_core;
    /// # use metrics_runtime::{Receiver, Measurement};
    /// # use metrics_core::Key;
    /// # use std::thread;
    /// # use std::time::Duration;
    /// # fn main() {
    /// let receiver = Receiver::builder().build().expect("failed to create receiver");
    /// let mut sink = receiver.sink();
    ///
    /// // A proxy is now registered under the name "load_stats", which is prepended to all the
    /// // metrics generated by the closure i.e. "load_stats.avg_1min".  These metrics are also
    /// // still scoped normally based on the [`Sink`].
    /// sink.proxy("load_stats", || {
    ///     let mut values = Vec::new();
    ///     values.push((Key::from_name("avg_1min"), Measurement::Gauge(19)));
    ///     values.push((Key::from_name("avg_5min"), Measurement::Gauge(12)));
    ///     values.push((Key::from_name("avg_10min"), Measurement::Gauge(10)));
    ///     values
    /// });
    /// # }
    /// ```
    pub fn proxy<N, F>(&mut self, name: N, f: F)
    where
        N: Into<Key>,
        F: Fn() -> Vec<(Key, Measurement)> + Send + Sync + 'static,
    {
        let id = Identifier::new(name.into(), self.scope_handle, Kind::Proxy);
        let handle = self.get_cached_value_handle(id);
        handle.update_proxy(f);
    }

    /// Creates a proxy metric, with labels attached.
    ///
    /// Proxy metrics allow you to register a closure that, when a snapshot of the metric state is
    /// requested, will be called and have a chance to return multiple metrics that are added to
    /// the overall metric of actual metrics.
    ///
    /// This can be useful for metrics which are expensive to constantly recalculate/poll, allowing
    /// you to avoid needing to calculate/push them them yourself, with all the boilerplate that
    /// comes with doing so periodically.
    ///
    /// Individual metrics must provide their own key (name), which will be appended to the name
    /// given when registering the proxy.  A proxy can be reregistered at any time by calling this
    /// function again with the same name.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate metrics_runtime;
    /// # extern crate metrics_core;
    /// # use metrics_runtime::{Receiver, Measurement};
    /// # use metrics_core::Key;
    /// # use std::thread;
    /// # use std::time::Duration;
    /// # fn main() {
    /// let receiver = Receiver::builder().build().expect("failed to create receiver");
    /// let mut sink = receiver.sink();
    ///
    /// let system_name = "web03".to_string();
    ///
    /// // A proxy is now registered under the name "load_stats", which is prepended to all the
    /// // metrics generated by the closure i.e. "load_stats.avg_1min".  These metrics are also
    /// // still scoped normally based on the [`Sink`].
    /// sink.proxy_with_labels("load_stats", &[("system", system_name)], || {
    ///     let mut values = Vec::new();
    ///     values.push((Key::from_name("avg_1min"), Measurement::Gauge(19)));
    ///     values.push((Key::from_name("avg_5min"), Measurement::Gauge(12)));
    ///     values.push((Key::from_name("avg_10min"), Measurement::Gauge(10)));
    ///     values
    /// });
    /// # }
    /// ```
    pub fn proxy_with_labels<N, L, F>(&mut self, name: N, labels: L, f: F)
    where
        N: Into<ScopedString>,
        L: IntoLabels,
        F: Fn() -> Vec<(Key, Measurement)> + Send + Sync + 'static,
    {
        self.proxy((name, labels), f)
    }

    pub(crate) fn construct_key<K>(&self, key: K) -> Key
    where
        K: Into<Key>,
    {
        let mut key = key.into();
        if !self.default_labels.is_empty() {
            key.add_labels(self.default_labels.clone());
        }
        key
    }

    fn get_owned_value_handle<K>(&mut self, key: K, kind: Kind) -> ValueHandle
    where
        K: Into<Key>,
    {
        let id = Identifier::new(key.into(), self.scope_handle, kind);
        self.get_cached_value_handle(id).clone()
    }

    fn get_cached_value_handle(&mut self, identifier: Identifier) -> &ValueHandle {
        // This gross hack gets around lifetime rules until full NLL is stable.  Without it, the
        // borrow checker doesn't understand the flow control and thinks the reference lives all
        // the way until the of the function, which breaks when we try to take a mutable reference
        // for inserting into the handle cache.
        if let Some(handle) = self.metric_cache.get(&identifier) {
            return unsafe { &*(handle as *const ValueHandle) };
        }

        let handle = self.metric_registry.get_or_register(identifier.clone());
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
            default_labels: self.default_labels.clone(),
        }
    }
}

impl<'a> AsScoped<'a> for str {
    fn as_scoped(&'a self, base: Scope) -> Scope {
        base.add_part(self.to_string())
    }
}

impl<'a, 'b, T> AsScoped<'a> for T
where
    &'a T: AsRef<[&'b str]>,
    T: 'a,
{
    fn as_scoped(&'a self, base: Scope) -> Scope {
        self.as_ref()
            .iter()
            .fold(base, |s, ss| s.add_part(ss.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::{Clock, MetricRegistry, Scope, ScopeRegistry, Sink};
    use crate::config::Configuration;
    use std::sync::Arc;

    #[test]
    fn test_construct_key() {
        // TODO(tobz): this is a lot of boilerplate to get a `Sink` for testing, wonder if there's
        // anything better we could be doing?
        let sregistry = Arc::new(ScopeRegistry::new());
        let config = Configuration::mock();
        let (clock, _) = Clock::mock();
        let mregistry = Arc::new(MetricRegistry::new(
            sregistry.clone(),
            config,
            clock.clone(),
        ));
        let mut sink = Sink::new(mregistry, sregistry, Scope::Root, clock);

        let no_labels = sink.construct_key("foo");
        assert_eq!(no_labels.name(), "foo");
        assert_eq!(no_labels.labels().count(), 0);

        let labels_given = sink.construct_key(("baz", &[("type", "test")]));
        assert_eq!(labels_given.name(), "baz");
        let label_str = labels_given
            .labels()
            .map(|l| format!("{}={}", l.key(), l.value()))
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(label_str, "type=test");

        sink.add_default_labels(&[(("service", "foo"))]);

        let no_labels = sink.construct_key("bar");
        assert_eq!(no_labels.name(), "bar");
        let label_str = no_labels
            .labels()
            .map(|l| format!("{}={}", l.key(), l.value()))
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(label_str, "service=foo");

        let labels_given = sink.construct_key(("quux", &[("type", "test")]));
        assert_eq!(labels_given.name(), "quux");
        let label_str = labels_given
            .labels()
            .map(|l| format!("{}={}", l.key(), l.value()))
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(label_str, "type=test,service=foo");
    }
}
