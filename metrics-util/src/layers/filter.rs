use crate::layers::Layer;
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, AhoCorasickKind};
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};

/// Filters and discards metrics matching certain name patterns.
///
/// More information on the behavior of the layer can be found in [`FilterLayer`].
pub struct Filter<R> {
    inner: R,
    automaton: AhoCorasick,
}

impl<R> Filter<R> {
    fn should_filter(&self, key: &str) -> bool {
        self.automaton.is_match(key)
    }
}

impl<R: Recorder> Recorder for Filter<R> {
    fn describe_counter(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        if self.should_filter(key_name.as_str()) {
            return;
        }
        self.inner.describe_counter(key_name, unit, description)
    }

    fn describe_gauge(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        if self.should_filter(key_name.as_str()) {
            return;
        }
        self.inner.describe_gauge(key_name, unit, description)
    }

    fn describe_histogram(&self, key_name: KeyName, unit: Option<Unit>, description: SharedString) {
        if self.should_filter(key_name.as_str()) {
            return;
        }
        self.inner.describe_histogram(key_name, unit, description)
    }

    fn register_counter(&self, key: &Key, metadata: &Metadata<'_>) -> Counter {
        if self.should_filter(key.name()) {
            return Counter::noop();
        }
        self.inner.register_counter(key, metadata)
    }

    fn register_gauge(&self, key: &Key, metadata: &Metadata<'_>) -> Gauge {
        if self.should_filter(key.name()) {
            return Gauge::noop();
        }
        self.inner.register_gauge(key, metadata)
    }

    fn register_histogram(&self, key: &Key, metadata: &Metadata<'_>) -> Histogram {
        if self.should_filter(key.name()) {
            return Histogram::noop();
        }
        self.inner.register_histogram(key, metadata)
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
            patterns: patterns.into_iter().map(|s| s.as_ref().to_string()).collect(),
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
    /// short patterns, or if you want the fastest possible search without regard to
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
            .kind(self.use_dfa.then_some(AhoCorasickKind::DFA))
            .build(&self.patterns)
            // Documentation for `AhoCorasickBuilder::build` states that the error here will be
            // related to exceeding some internal limits, but that those limits should generally be
            // large enough for most use cases.. so I'm making the executive decision to consider
            // that "good enough" and treat this as an exceptional error if it does occur.
            .expect("should not fail to build filter automaton");
        Filter { inner, automaton }
    }
}

#[cfg(test)]
mod tests {
    use super::FilterLayer;
    use crate::{layers::Layer, test_util::*};
    use metrics::{Counter, Gauge, Histogram, Unit};

    static METADATA: metrics::Metadata =
        metrics::Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));

    #[test]
    fn test_basic_functionality() {
        let inputs = vec![
            RecorderOperation::DescribeCounter(
                "tokio.loops".into(),
                Some(Unit::Count),
                "counter desc".into(),
            ),
            RecorderOperation::DescribeGauge(
                "hyper.bytes_read".into(),
                Some(Unit::Bytes),
                "gauge desc".into(),
            ),
            RecorderOperation::DescribeHistogram(
                "hyper.response_latency".into(),
                Some(Unit::Nanoseconds),
                "histogram desc".into(),
            ),
            RecorderOperation::DescribeCounter(
                "tokio.spurious_wakeups".into(),
                Some(Unit::Count),
                "counter desc".into(),
            ),
            RecorderOperation::DescribeGauge(
                "bb8.pooled_conns".into(),
                Some(Unit::Count),
                "gauge desc".into(),
            ),
            RecorderOperation::RegisterCounter("tokio.loops".into(), Counter::noop(), &METADATA),
            RecorderOperation::RegisterGauge("hyper.bytes_read".into(), Gauge::noop(), &METADATA),
            RecorderOperation::RegisterHistogram(
                "hyper.response_latency".into(),
                Histogram::noop(),
                &METADATA,
            ),
            RecorderOperation::RegisterCounter(
                "tokio.spurious_wakeups".into(),
                Counter::noop(),
                &METADATA,
            ),
            RecorderOperation::RegisterGauge("bb8.pooled_conns".into(), Gauge::noop(), &METADATA),
        ];

        let expectations = vec![
            RecorderOperation::DescribeGauge(
                "hyper.bytes_read".into(),
                Some(Unit::Bytes),
                "gauge desc".into(),
            ),
            RecorderOperation::DescribeHistogram(
                "hyper.response_latency".into(),
                Some(Unit::Nanoseconds),
                "histogram desc".into(),
            ),
            RecorderOperation::RegisterGauge("hyper.bytes_read".into(), Gauge::noop(), &METADATA),
            RecorderOperation::RegisterHistogram(
                "hyper.response_latency".into(),
                Histogram::noop(),
                &METADATA,
            ),
        ];

        let recorder = MockBasicRecorder::from_operations(expectations);
        let filter = FilterLayer::from_patterns(&["tokio", "bb8"]);
        let filter = filter.layer(recorder);

        for operation in inputs {
            operation.apply_to_recorder(&filter);
        }
    }

    #[test]
    fn test_case_insensitivity() {
        let inputs = vec![
            RecorderOperation::DescribeCounter(
                "tokiO.loops".into(),
                Some(Unit::Count),
                "counter desc".into(),
            ),
            RecorderOperation::DescribeGauge(
                "hyper.bytes_read".into(),
                Some(Unit::Bytes),
                "gauge desc".into(),
            ),
            RecorderOperation::DescribeHistogram(
                "hyper.response_latency".into(),
                Some(Unit::Nanoseconds),
                "histogram desc".into(),
            ),
            RecorderOperation::DescribeCounter(
                "Tokio.spurious_wakeups".into(),
                Some(Unit::Count),
                "counter desc".into(),
            ),
            RecorderOperation::DescribeGauge(
                "bB8.pooled_conns".into(),
                Some(Unit::Count),
                "gauge desc".into(),
            ),
            RecorderOperation::RegisterCounter("tokiO.loops".into(), Counter::noop(), &METADATA),
            RecorderOperation::RegisterGauge("hyper.bytes_read".into(), Gauge::noop(), &METADATA),
            RecorderOperation::RegisterHistogram(
                "hyper.response_latency".into(),
                Histogram::noop(),
                &METADATA,
            ),
            RecorderOperation::RegisterCounter(
                "Tokio.spurious_wakeups".into(),
                Counter::noop(),
                &METADATA,
            ),
            RecorderOperation::RegisterGauge("bB8.pooled_conns".into(), Gauge::noop(), &METADATA),
        ];

        let expectations = vec![
            RecorderOperation::DescribeGauge(
                "hyper.bytes_read".into(),
                Some(Unit::Bytes),
                "gauge desc".into(),
            ),
            RecorderOperation::DescribeHistogram(
                "hyper.response_latency".into(),
                Some(Unit::Nanoseconds),
                "histogram desc".into(),
            ),
            RecorderOperation::RegisterGauge("hyper.bytes_read".into(), Gauge::noop(), &METADATA),
            RecorderOperation::RegisterHistogram(
                "hyper.response_latency".into(),
                Histogram::noop(),
                &METADATA,
            ),
        ];

        let recorder = MockBasicRecorder::from_operations(expectations);
        let mut filter = FilterLayer::from_patterns(&["tokio", "bb8"]);
        let filter = filter.case_insensitive(true).layer(recorder);

        for operation in inputs {
            operation.apply_to_recorder(&filter);
        }
    }
}
