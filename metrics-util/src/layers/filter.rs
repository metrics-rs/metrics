use crate::layers::Layer;
use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use metrics::{GaugeValue, Key, Recorder, Unit};

/// Filters and discards metrics matching certain name patterns.
///
/// More information on the behavior of the layer can be found in [`FilterLayer`].
pub struct Filter<R> {
    inner: R,
    automaton: AhoCorasick,
}

impl<R> Filter<R> {
    fn should_filter(&self, key: &Key) -> bool {
        key.name()
            .parts()
            .any(|s| self.automaton.is_match(s.as_ref()))
    }
}

impl<R: Recorder> Recorder for Filter<R> {
    fn register_counter(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        if self.should_filter(&key) {
            return;
        }
        self.inner.register_counter(key, unit, description)
    }

    fn register_gauge(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        if self.should_filter(&key) {
            return;
        }
        self.inner.register_gauge(key, unit, description)
    }

    fn register_histogram(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        if self.should_filter(&key) {
            return;
        }
        self.inner.register_histogram(key, unit, description)
    }

    fn increment_counter(&self, key: &Key, value: u64) {
        if self.should_filter(&key) {
            return;
        }
        self.inner.increment_counter(key, value);
    }

    fn update_gauge(&self, key: &Key, value: GaugeValue) {
        if self.should_filter(&key) {
            return;
        }
        self.inner.update_gauge(key, value);
    }

    fn record_histogram(&self, key: &Key, value: f64) {
        if self.should_filter(&key) {
            return;
        }
        self.inner.record_histogram(key, value);
    }
}

/// A layer for filtering and discarding metrics matching certain name patterns.
///
/// Uses an [Aho-Corasick][ahocorasick] automaton to efficiently match a metric key against
/// multiple patterns at once.  Patterns are matched across the entire key i.e. they are
/// matched as substrings.
///
/// If a metric key matches any of the configured patterns, it will be skipped entirely.  This
/// applies equally to metric registration and metric emission.
///
/// A number of options are exposed that control the underlying automaton, such as compilation to a
/// DFA, or case sensitivity.
///
/// [ahocorasick]: https://en.wikipedia.org/wiki/Aho–Corasick_algorithm
#[derive(Default)]
pub struct FilterLayer {
    patterns: Vec<String>,
    case_insensitive: bool,
    use_dfa: bool,
}

impl FilterLayer {
    /// Creates a [`FilterLayer`] from an existing set of patterns.
    pub fn from_patterns<P, I>(patterns: P) -> Self
    where
        P: IntoIterator<Item = I>,
        I: AsRef<str>,
    {
        FilterLayer {
            patterns: patterns
                .into_iter()
                .map(|s| s.as_ref().to_string())
                .collect(),
            case_insensitive: false,
            use_dfa: true,
        }
    }

    /// Adds a pattern to match.
    pub fn add_pattern<P>(&mut self, pattern: P) -> &mut FilterLayer
    where
        P: AsRef<str>,
    {
        self.patterns.push(pattern.as_ref().to_string());
        self
    }

    /// Sets the case sensitivity used for pattern matching.
    ///
    /// Defaults to `false` i.e. searches are case sensitive.
    pub fn case_insensitive(&mut self, case_insensitive: bool) -> &mut FilterLayer {
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
    pub fn use_dfa(&mut self, dfa: bool) -> &mut FilterLayer {
        self.use_dfa = dfa;
        self
    }
}

impl<R> Layer<R> for FilterLayer {
    type Output = Filter<R>;

    fn layer(&self, inner: R) -> Self::Output {
        let mut automaton_builder = AhoCorasickBuilder::new();
        let automaton = automaton_builder
            .ascii_case_insensitive(self.case_insensitive)
            .dfa(self.use_dfa)
            .auto_configure(&self.patterns)
            .build(&self.patterns);
        Filter { inner, automaton }
    }
}

#[cfg(test)]
mod tests {
    use super::FilterLayer;
    use crate::debugging::DebuggingRecorder;
    use crate::layers::Layer;
    use metrics::{Recorder, Unit};

    #[test]
    fn test_basic_functionality() {
        let patterns = &["tokio", "bb8"];
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let filter = FilterLayer::from_patterns(patterns);
        let layered = filter.layer(recorder);

        let tlkey = "tokio.loops".into();
        let hsbkey = "hyper.sent_bytes".into();
        let htsbkey = "hyper.tokio.sent_bytes".into();
        let bckey = "bb8.conns".into();
        let hrbkey = "hyper.recv_bytes".into();

        let before = snapshotter.snapshot();
        assert_eq!(before.len(), 0);

        let ud = &[
            (Unit::Count, "counter desc"),
            (Unit::Bytes, "gauge desc"),
            (Unit::Bytes, "histogram desc"),
            (Unit::Count, "counter desc"),
            (Unit::Bytes, "gauge desc"),
        ];

        layered.register_counter(&tlkey, Some(ud[0].0.clone()), Some(ud[0].1));
        layered.register_gauge(&hsbkey, Some(ud[1].0.clone()), Some(ud[1].1));
        layered.register_histogram(&htsbkey, Some(ud[2].0.clone()), Some(ud[2].1));
        layered.register_counter(&bckey, Some(ud[3].0.clone()), Some(ud[3].1));
        layered.register_gauge(&hrbkey, Some(ud[4].0.clone()), Some(ud[4].1));

        let after = snapshotter.snapshot();
        assert_eq!(after.len(), 2);

        for (key, unit, desc, _value) in after {
            assert!(
                !key.key().name().to_string().contains("tokio")
                    && !key.key().name().to_string().contains("bb8")
            );
            // We cheat here since we're not comparing one-to-one with the source data,
            // but we know which metrics are going to make it through so we can hard code.
            assert_eq!(Some(Unit::Bytes), unit);
            assert!(!desc.unwrap().is_empty() && desc.unwrap() == "gauge desc");
        }
    }

    #[test]
    fn test_case_insensitivity() {
        let patterns = &["tokio", "bb8"];
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let mut filter = FilterLayer::from_patterns(patterns.iter());
        filter.case_insensitive(true);
        let layered = filter.layer(recorder);

        let before = snapshotter.snapshot();
        assert_eq!(before.len(), 0);

        let tlkey = "tokiO.loops".into();
        let hsbkey = "hyper.sent_bytes".into();
        let hrbkey = "hyper.recv_bytes".into();
        let bckey = "bb8.conns".into();
        let bcckey = "Bb8.conns_closed".into();

        layered.register_counter(&tlkey, None, None);
        layered.register_gauge(&hsbkey, None, None);
        layered.register_histogram(&hrbkey, None, None);
        layered.register_counter(&bckey, None, None);
        layered.register_counter(&bcckey, None, None);

        let after = snapshotter.snapshot();
        assert_eq!(after.len(), 2);

        for (key, _unit, _desc, _value) in &after {
            assert!(
                !key.key()
                    .name()
                    .to_string()
                    .to_lowercase()
                    .contains("tokio")
                    && !key.key().name().to_string().to_lowercase().contains("bb8")
            );
        }
    }
}
