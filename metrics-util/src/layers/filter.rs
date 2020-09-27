use crate::layers::Layer;
use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use metrics::{Key, Recorder};

/// Filters and discards metrics matching certain name patterns.
///
/// Uses an Aho-Corasick automaton to efficiently match a metric key against multiple patterns at
/// once.  Patterns are matched across the entire key i.e. they are matched as substrings.
pub struct Filter<R> {
    inner: R,
    automaton: AhoCorasick,
}

impl<R> Filter<R> {
    fn should_filter(&self, key: &Key) -> bool {
        self.automaton.is_match(key.name().as_ref())
    }
}

impl<R: Recorder> Recorder for Filter<R> {
    fn register_counter(&self, key: Key, description: Option<&'static str>) {
        if self.should_filter(&key) {
            return;
        }
        self.inner.register_counter(key, description)
    }

    fn register_gauge(&self, key: Key, description: Option<&'static str>) {
        if self.should_filter(&key) {
            return;
        }
        self.inner.register_gauge(key, description)
    }

    fn register_histogram(&self, key: Key, description: Option<&'static str>) {
        if self.should_filter(&key) {
            return;
        }
        self.inner.register_histogram(key, description)
    }

    fn increment_counter(&self, key: Key, value: u64) {
        if self.should_filter(&key) {
            return;
        }
        self.inner.increment_counter(key, value);
    }

    fn update_gauge(&self, key: Key, value: f64) {
        if self.should_filter(&key) {
            return;
        }
        self.inner.update_gauge(key, value);
    }

    fn record_histogram(&self, key: Key, value: u64) {
        if self.should_filter(&key) {
            return;
        }
        self.inner.record_histogram(key, value);
    }
}

/// A layer for filtering and discarding metrics matching certain name patterns.
///
/// More information on the behavior of the layer can be found in [`Filter`].
#[derive(Default)]
pub struct FilterLayer {
    patterns: Vec<String>,
    case_insensitive: bool,
    use_dfa: bool,
}

impl FilterLayer {
    /// Creates a `FilterLayer` from an existing set of patterns.
    pub fn from_patterns<P, I>(patterns: P) -> Self
    where
        P: Iterator<Item = I>,
        I: AsRef<str>,
    {
        FilterLayer {
            patterns: patterns.map(|s| s.as_ref().to_string()).collect(),
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
    /// Per the docs for the `aho-corasick` crate, enabling the DFA trades off space usage and build
    /// time (at runtime, not compile time) in order to reduce the search time complexity.  Using
    /// the DFA is beneficial when matching a small number of short patterns, which should be fairly
    /// common when filtering metrics.
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
    use metrics::{Key, Recorder};

    #[test]
    fn test_basic_functionality() {
        let patterns = &["tokio", "bb8"];
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let filter = FilterLayer::from_patterns(patterns.iter());
        let layered = filter.layer(recorder);

        let before = snapshotter.snapshot();
        assert_eq!(before.len(), 0);

        layered.register_counter(Key::Owned("tokio.loops".into()), None);
        layered.register_gauge(Key::Owned("hyper.sent_bytes".into()), None);
        layered.register_histogram(Key::Owned("hyper.recv_bytes".into()), None);
        layered.register_counter(Key::Owned("bb8.conns".into()), None);
        layered.register_gauge(Key::Owned("hyper.tokio.sent_bytes".into()), None);

        let after = snapshotter.snapshot();
        assert_eq!(after.len(), 2);

        for (_kind, key, _value) in &after {
            assert!(!key.name().contains("tokio") && !key.name().contains("bb8"));
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

        layered.register_counter(Key::Owned("tokiO.loops".into()), None);
        layered.register_gauge(Key::Owned("hyper.sent_bytes".into()), None);
        layered.register_histogram(Key::Owned("hyper.recv_bytes".into()), None);
        layered.register_counter(Key::Owned("bb8.conns".into()), None);
        layered.register_counter(Key::Owned("Bb8.conns_closed".into()), None);

        let after = snapshotter.snapshot();
        assert_eq!(after.len(), 2);

        for (_kind, key, _value) in &after {
            assert!(
                !key.name().to_lowercase().contains("tokio")
                    && !key.name().to_lowercase().contains("bb8")
            );
        }
    }
}
