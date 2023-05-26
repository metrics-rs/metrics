use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};
use radix_trie::{Trie, TrieCommon};

use crate::{MetricKind, MetricKindMask};

/// Routes metrics to specific target recorders.
///
/// More information on the behavior of the layer can be found in [`RouterBuilder`].
pub struct Router {
    default: Box<dyn Recorder>,
    global_mask: MetricKindMask,
    targets: Vec<Box<dyn Recorder>>,
    counter_routes: Trie<String, usize>,
    gauge_routes: Trie<String, usize>,
    histogram_routes: Trie<String, usize>,
}

impl Router {
    fn route(
        &self,
        kind: MetricKind,
        key: &str,
        search_routes: &Trie<String, usize>,
    ) -> &dyn Recorder {
        // The global mask is essentially a Bloom filter of overridden route types.  If it doesn't
        // match our metric, we know for a fact there's no route and must use the default recorder.
        if !self.global_mask.matches(kind) {
            self.default.as_ref()
        } else {
            // SAFETY: We derive the `idx` value that is inserted into our route maps by using the
            // length of `targets` itself before adding a new target.  Ergo, the index is provably
            // populated if the `idx` has been stored.
            search_routes
                .get_ancestor(key)
                .map(|st| unsafe { self.targets.get_unchecked(*st.value().unwrap()).as_ref() })
                .unwrap_or_else(|| self.default.as_ref())
        }
    }
}

impl Recorder for Router {
    fn describe_counter(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        let target = self.route(MetricKind::Counter, key_name.as_str(), &self.counter_routes);
        target.describe_counter(key_name, unit, description)
    }

    fn describe_gauge(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        let target = self.route(MetricKind::Gauge, key_name.as_str(), &self.gauge_routes);
        target.describe_gauge(key_name, unit, description)
    }

    fn describe_histogram(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        let target = self.route(MetricKind::Histogram, key_name.as_str(), &self.histogram_routes);
        target.describe_histogram(key_name, unit, description)
    }

    fn register_counter(&self, key: &Key, metadata: &Metadata<'_>) -> Counter {
        let target = self.route(MetricKind::Counter, key.name(), &self.counter_routes);
        target.register_counter(key, metadata)
    }

    fn register_gauge(&self, key: &Key, metadata: &Metadata<'_>) -> Gauge {
        let target = self.route(MetricKind::Gauge, key.name(), &self.gauge_routes);
        target.register_gauge(key, metadata)
    }

    fn register_histogram(&self, key: &Key, metadata: &Metadata<'_>) -> Histogram {
        let target = self.route(MetricKind::Histogram, key.name(), &self.histogram_routes);
        target.register_histogram(key, metadata)
    }
}

/// Routes metrics to specific target recorders.
///
/// Routes are defined as a prefix to check against the metric name, and a mask for the metric type.
/// For example,  a route with the pattern of "foo" would match "foo", "or "foo.submetric", but not
/// "something.foo". Likewise, a metric mask of "all" would apply this route to counters, gauges,
/// and histograms, while any specific mask would only apply to the given metric kind.
///
/// A default route (recorder) is always present and used in the case that no specific route exists.
pub struct RouterBuilder {
    default: Box<dyn Recorder>,
    global_mask: MetricKindMask,
    targets: Vec<Box<dyn Recorder>>,
    counter_routes: Trie<String, usize>,
    gauge_routes: Trie<String, usize>,
    histogram_routes: Trie<String, usize>,
}

impl RouterBuilder {
    /// Creates a [`RouterBuilder`] from a [`Recorder`].
    ///
    /// The given recorder is used as the default route when no other specific route exists.
    pub fn from_recorder<R>(recorder: R) -> Self
    where
        R: Recorder + 'static,
    {
        RouterBuilder {
            default: Box::new(recorder),
            global_mask: MetricKindMask::NONE,
            targets: Vec::new(),
            counter_routes: Trie::new(),
            gauge_routes: Trie::new(),
            histogram_routes: Trie::new(),
        }
    }

    /// Adds a route.
    ///
    /// `mask` defines which metric kinds will match the given route, and `pattern` is a prefix
    /// string used to match against metric names.
    ///
    /// If a matching route already exists, it will be overwritten.
    pub fn add_route<P, R>(
        &mut self,
        mask: MetricKindMask,
        pattern: P,
        recorder: R,
    ) -> &mut RouterBuilder
    where
        P: AsRef<str>,
        R: Recorder + 'static,
    {
        let target_idx = self.targets.len();
        self.targets.push(Box::new(recorder));

        self.global_mask = self.global_mask | mask;

        match mask {
            MetricKindMask::ALL => {
                let _ = self.counter_routes.insert(pattern.as_ref().to_string(), target_idx);
                let _ = self.gauge_routes.insert(pattern.as_ref().to_string(), target_idx);
                let _ = self.histogram_routes.insert(pattern.as_ref().to_string(), target_idx);
            }
            MetricKindMask::COUNTER => {
                let _ = self.counter_routes.insert(pattern.as_ref().to_string(), target_idx);
            }
            MetricKindMask::GAUGE => {
                let _ = self.gauge_routes.insert(pattern.as_ref().to_string(), target_idx);
            }
            MetricKindMask::HISTOGRAM => {
                let _ = self.histogram_routes.insert(pattern.as_ref().to_string(), target_idx);
            }
            _ => panic!("cannot add route for unknown or empty metric kind mask"),
        };
        self
    }

    /// Builds the configured [`Router`].
    pub fn build(self) -> Router {
        Router {
            default: self.default,
            global_mask: self.global_mask,
            targets: self.targets,
            counter_routes: self.counter_routes,
            gauge_routes: self.gauge_routes,
            histogram_routes: self.histogram_routes,
        }
    }
}

#[cfg(test)]
mod tests {
    use mockall::{
        mock,
        predicate::{always, eq},
        Sequence,
    };
    use std::borrow::Cow;

    use super::RouterBuilder;
    use crate::MetricKindMask;
    use metrics::{
        Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit,
    };

    mock! {
        pub TestRecorder {
        }

        impl Recorder for TestRecorder {
            fn describe_counter(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString);
            fn describe_gauge(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString);
            fn describe_histogram(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString);
            fn register_counter<'a>(&'a self, key: &'a Key, metadata: &'a Metadata<'a>) -> Counter;
            fn register_gauge<'a>(&'a self, key: &'a Key, metadata: &'a Metadata<'a>) -> Gauge;
            fn register_histogram<'a>(&'a self, key: &'a Key, metadata: &'a Metadata<'a>) -> Histogram;
        }
    }

    #[test]
    fn test_construction() {
        let _ = RouterBuilder::from_recorder(MockTestRecorder::new()).build();

        let mut builder = RouterBuilder::from_recorder(MockTestRecorder::new());
        builder
            .add_route(MetricKindMask::COUNTER, "foo", MockTestRecorder::new())
            .add_route(MetricKindMask::GAUGE, "bar".to_owned(), MockTestRecorder::new())
            .add_route(MetricKindMask::HISTOGRAM, Cow::Borrowed("baz"), MockTestRecorder::new())
            .add_route(MetricKindMask::ALL, "quux", MockTestRecorder::new());
        let _ = builder.build();
    }

    #[test]
    #[should_panic]
    fn test_bad_construction() {
        let mut builder = RouterBuilder::from_recorder(MockTestRecorder::new());
        builder.add_route(MetricKindMask::NONE, "foo", MockTestRecorder::new());
        let _ = builder.build();
    }

    #[test]
    fn test_basic_functionality() {
        let default_counter: Key = "counter_default.foo".into();
        let override_counter: Key = "counter_override.foo".into();
        let all_override: Key = "all_override.foo".into();

        let mut default_mock = MockTestRecorder::new();
        let mut counter_mock = MockTestRecorder::new();
        let mut all_mock = MockTestRecorder::new();

        let mut seq = Sequence::new();

        static METADATA: metrics::Metadata =
            metrics::Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));

        default_mock
            .expect_register_counter()
            .times(1)
            .in_sequence(&mut seq)
            .with(eq(default_counter.clone()), always())
            .returning(|_, _| Counter::noop());

        counter_mock
            .expect_register_counter()
            .times(1)
            .in_sequence(&mut seq)
            .with(eq(override_counter.clone()), always())
            .returning(|_, _| Counter::noop());

        all_mock
            .expect_register_counter()
            .times(1)
            .in_sequence(&mut seq)
            .with(eq(all_override.clone()), always())
            .returning(|_, _| Counter::noop());

        all_mock
            .expect_register_histogram()
            .times(1)
            .in_sequence(&mut seq)
            .with(eq(all_override.clone()), always())
            .returning(|_, _| Histogram::noop());

        let mut builder = RouterBuilder::from_recorder(default_mock);
        builder.add_route(MetricKindMask::COUNTER, "counter_override", counter_mock).add_route(
            MetricKindMask::ALL,
            "all_override",
            all_mock,
        );
        let recorder = builder.build();

        let _ = recorder.register_counter(&default_counter, &METADATA);
        let _ = recorder.register_counter(&override_counter, &METADATA);
        let _ = recorder.register_counter(&all_override, &METADATA);
        let _ = recorder.register_histogram(&all_override, &METADATA);
    }
}
