use std::collections::HashMap;

use crate::layers::Layer;
use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use metrics::{GaugeValue, Key, Recorder, Unit};
use parking_lot::Mutex;

/// Converts absolute counter values into incremental values.
///
/// More information on the behavior of the layer can be found in [`AbsoluteLayer`].
pub struct Absolute<R> {
    inner: R,
    automaton: AhoCorasick,
    seen: Mutex<HashMap<Key, u64>>,
}

impl<R> Absolute<R> {
    fn should_convert(&self, key: &Key) -> bool {
        key.name()
            .parts()
            .any(|s| self.automaton.is_match(s.as_ref()))
    }
}

impl<R: Recorder> Recorder for Absolute<R> {
    fn register_counter(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        self.inner.register_counter(key, unit, description)
    }

    fn register_gauge(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        self.inner.register_gauge(key, unit, description)
    }

    fn register_histogram(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        self.inner.register_histogram(key, unit, description)
    }

    fn increment_counter(&self, key: &Key, value: u64) {
        let value = if self.should_convert(&key) {
            let mut seen = self.seen.lock();
            let curr_value = seen.entry(key.clone()).or_default();
            if value <= *curr_value {
                return;
            }

            let delta = value - *curr_value;
            *curr_value = value;
            delta
        } else {
            value
        };
        self.inner.increment_counter(key, value);
    }

    fn update_gauge(&self, key: &Key, value: GaugeValue) {
        self.inner.update_gauge(key, value);
    }

    fn record_histogram(&self, key: &Key, value: f64) {
        self.inner.record_histogram(key, value);
    }
}

/// A layer for converting absolute counter values into incremental values.
///
/// In some systems, metrics are handled externally, meaning that users only have access to
/// point-in-time snapshots of the values.  This requires users to track the last value and the
/// current value for the purposes of incrementing counters by the delta.  Holding on to this
/// data in each location is cumbersome.
///
/// `AbsoluteLayer` instead tracks all of this data in a single location, driven by specific metric
/// name patterns.  If a metric matches a given pattern, the layer treats it as an absolute value.
/// It will figure out the delta between the last known value and the current value, passing only
/// that delta along to the next layer.  If the current value is not monotonic with respect to the
/// last known value, then no value will be emitted whatsoever.  This preserves the invariant of
/// counters being monotonic.
///
/// Only counters are converted, and all other metric types are passed through unchanged.
/// Internally, the state which holds key/value associations is protected by a mutex, so this layer
/// is not suitable for updating absolute counters that are emitted at a very high frequency or a
/// very large number of concurrent emissions.
///
/// Uses an [Aho-Corasick][ahocorasick] automaton to efficiently match a metric key against
/// multiple patterns at once.  Patterns are matched across the entire key i.e. they are
/// matched as substrings.
///
/// A number of options are exposed that control the underlying automaton, such as compilation to a
/// DFA, or case sensitivity.
///
/// [ahocorasick]: https://en.wikipedia.org/wiki/Aho–Corasick_algorithm
#[derive(Default)]
pub struct AbsoluteLayer {
    patterns: Vec<String>,
    case_insensitive: bool,
    use_dfa: bool,
}

impl AbsoluteLayer {
    /// Creates a [`AbsoluteLayer`] from an existing set of patterns.
    pub fn from_patterns<P, I>(patterns: P) -> Self
    where
        P: IntoIterator<Item = I>,
        I: AsRef<str>,
    {
        AbsoluteLayer {
            patterns: patterns
                .into_iter()
                .map(|s| s.as_ref().to_string())
                .collect(),
            case_insensitive: false,
            use_dfa: true,
        }
    }

    /// Adds a pattern to match.
    pub fn add_pattern<P>(&mut self, pattern: P) -> &mut AbsoluteLayer
    where
        P: AsRef<str>,
    {
        self.patterns.push(pattern.as_ref().to_string());
        self
    }

    /// Sets the case sensitivity used for pattern matching.
    ///
    /// Defaults to `false` i.e. searches are case sensitive.
    pub fn case_insensitive(&mut self, case_insensitive: bool) -> &mut AbsoluteLayer {
        self.case_insensitive = case_insensitive;
        self
    }

    /// Sets whether or not to internally use a deterministic finite automaton.
    ///
    /// The main benefit to a DFA is that it can execute searches more quickly than a NFA (perhaps
    /// 2-4 times as fast). The main drawback is that the DFA uses more space and can take much
    /// longer to build.
    ///
    /// Enabling this option does not change the time complexity for constructing the underlying
    /// Aho-Corasick automaton (which is O(p) where p is the total number of patterns being
    /// compiled). Enabling this option does however reduce the time complexity of non-overlapping
    /// searches from O(n + p) to O(n), where n is the length of the haystack.
    ///
    /// In general, it's a good idea to enable this if you're searching a small number of fairly
    /// short patterns (~1000), or if you want the fastest possible search without regard to
    /// compilation time or space usage.
    ///
    /// Defaults to `true`.
    pub fn use_dfa(&mut self, dfa: bool) -> &mut AbsoluteLayer {
        self.use_dfa = dfa;
        self
    }
}

impl<R> Layer<R> for AbsoluteLayer {
    type Output = Absolute<R>;

    fn layer(&self, inner: R) -> Self::Output {
        let mut automaton_builder = AhoCorasickBuilder::new();
        let automaton = automaton_builder
            .ascii_case_insensitive(self.case_insensitive)
            .dfa(self.use_dfa)
            .auto_configure(&self.patterns)
            .build(&self.patterns);
        Absolute {
            inner,
            automaton,
            seen: Mutex::new(HashMap::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AbsoluteLayer;
    use crate::layers::Layer;
    use crate::{debugging::DebuggingRecorder, DebugValue};
    use metrics::{GaugeValue, Key, Recorder};
    use ordered_float::OrderedFloat;

    #[test]
    fn test_basic_functionality() {
        let patterns = &["rdkafka"];
        let recorder = DebuggingRecorder::with_ordering(true);
        let snapshotter = recorder.snapshotter();
        let absolute = AbsoluteLayer::from_patterns(patterns);
        let layered = absolute.layer(recorder);

        let ckey = "counter".into();
        let gkey = "gauge".into();
        let hkey = "histo".into();

        let before = snapshotter.snapshot();
        assert_eq!(before.len(), 0);

        layered.register_counter(&ckey, None, None);
        layered.register_gauge(&gkey, None, None);
        layered.register_histogram(&hkey, None, None);

        let after = snapshotter.snapshot();
        assert_eq!(after.len(), 3);

        assert_eq!(after[0].0.key(), &ckey);
        assert_eq!(after[1].0.key(), &gkey);
        assert_eq!(after[2].0.key(), &hkey);

        layered.increment_counter(&ckey, 42);
        layered.update_gauge(&gkey, GaugeValue::Absolute(-420.69));
        layered.record_histogram(&hkey, 3.14);

        let after = snapshotter.snapshot();
        assert_eq!(after.len(), 3);

        assert_eq!(after[0].0.key(), &ckey);
        assert_eq!(after[0].3, DebugValue::Counter(42));
        assert_eq!(after[1].0.key(), &gkey);
        assert_eq!(after[1].3, DebugValue::Gauge(OrderedFloat::<f64>(-420.69)));
        assert_eq!(after[2].0.key(), &hkey);
        assert_eq!(
            after[2].3,
            DebugValue::Histogram(vec![OrderedFloat::<f64>(3.14)])
        );
    }

    #[test]
    fn test_absolute_to_delta() {
        let patterns = &["rdkafka"];
        let recorder = DebuggingRecorder::with_ordering(true);
        let snapshotter = recorder.snapshotter();
        let absolute = AbsoluteLayer::from_patterns(patterns);
        let layered = absolute.layer(recorder);

        let ckey = "counter".into();
        let rbkey = "rdkafka.bytes".into();

        let before = snapshotter.snapshot();
        assert_eq!(before.len(), 0);

        // First counter.  Brand new.
        layered.increment_counter(&ckey, 42);

        let after = snapshotter.snapshot();
        assert_eq!(after.len(), 1);

        assert_eq!(after[0].0.key(), &ckey);
        assert_eq!(after[0].3, DebugValue::Counter(42));

        // Second counter.  Brand new.
        layered.increment_counter(&rbkey, 18);

        let after = snapshotter.snapshot();
        assert_eq!(after.len(), 2);

        assert_eq!(after[0].0.key(), &ckey);
        assert_eq!(after[0].3, DebugValue::Counter(42));
        assert_eq!(after[1].0.key(), &rbkey);
        assert_eq!(after[1].3, DebugValue::Counter(18));

        // Now do them both.
        layered.increment_counter(&ckey, 42);
        layered.increment_counter(&rbkey, 18);

        let after = snapshotter.snapshot();
        assert_eq!(after.len(), 2);

        assert_eq!(after[0].0.key(), &ckey);
        assert_eq!(after[0].3, DebugValue::Counter(84));
        assert_eq!(after[1].0.key(), &rbkey);
        assert_eq!(after[1].3, DebugValue::Counter(18));

        // Try setting another absolute value.
        layered.increment_counter(&rbkey, 24);

        let after = snapshotter.snapshot();
        assert_eq!(after.len(), 2);

        assert_eq!(after[1].0.key(), &rbkey);
        assert_eq!(after[1].3, DebugValue::Counter(24));

        // And make certain we can't regress.
        layered.increment_counter(&rbkey, 18);

        let after = snapshotter.snapshot();
        assert_eq!(after.len(), 2);

        assert_eq!(after[1].0.key(), &rbkey);
        assert_eq!(after[1].3, DebugValue::Counter(24));
    }
}
