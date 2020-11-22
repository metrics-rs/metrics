//! Records metrics in the Prometheus exposition format.
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg), deny(broken_intra_doc_links))]
use std::future::Future;

#[cfg(feature = "tokio-exporter")]
use hyper::{
    service::{make_service_fn, service_fn},
    {Body, Error as HyperError, Response, Server},
};
use metrics::{Key, Recorder, SetRecorderError, Unit};
use metrics_util::{
    parse_quantiles, CompositeKey, Handle, Histogram, MetricKind, Quantile, Registry, Generation
};
use parking_lot::{Mutex, RwLock};
use quanta::{Clock, Instant};
use std::collections::HashMap;
use std::io;
use std::iter::FromIterator;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::ops::BitOr;
use std::ops::DerefMut;
use std::sync::Arc;
#[cfg(feature = "tokio-exporter")]
use std::thread;
use std::time::Duration;
use thiserror::Error as ThisError;
#[cfg(feature = "tokio-exporter")]
use tokio::{pin, runtime, select};

type PrometheusRegistry = Registry<CompositeKey, Handle>;
type HdrHistogram = hdrhistogram::Histogram<u64>;

/// Metric type.
///
/// Used for configuring which metrics should be culled when idle timeouts are configured.
#[derive(Debug, PartialEq)]
pub struct MetricType(u8);

impl MetricType {
    /// No metrics will be eligible for culling.
    pub const NONE: MetricType = MetricType(0);

    /// Counters will be eligible for culling.
    pub const COUNTER: MetricType = MetricType(1);

    /// Gauges will be eligible for culling.
    pub const GAUGE: MetricType = MetricType(2);

    /// Histograms will be eligible for culling.
    pub const HISTOGRAM: MetricType = MetricType(4);

    /// All metrics will be eligible for culling.
    #[allow(dead_code)]
    pub const ALL: MetricType = MetricType(7);

    pub(crate) fn has(&self, other: MetricType) -> bool {
        self.0 & other.0 != 0
    }
}

impl BitOr for MetricType {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

/// Matches a metric name in a specific way.
///
/// Used for specifying overrides for buckets, allowing a default set of histogram buckets to be
/// specified while adjusting the buckets that get used for specific metrics.
#[derive(Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub enum Matcher {
    /// Matches the entire metric name.
    Full(String),
    /// Matches the beginning of the metric name.
    Prefix(String),
    /// Matches the end of the metric name.
    Suffix(String),
}

impl Matcher {
    /// Checks if the given key matches this matcher.
    pub fn matches(&self, key: &str) -> bool {
        match self {
            Matcher::Prefix(prefix) => key.starts_with(prefix),
            Matcher::Suffix(suffix) => key.ends_with(suffix),
            Matcher::Full(full) => key == full,
        }
    }
}

/// Errors that could occur while installing a Prometheus recorder/exporter.
#[derive(ThisError, Debug)]
pub enum Error {
    /// Creating the networking event loop did not succeed.
    #[error("failed to spawn Tokio runtime for endpoint: {0}")]
    Io(#[from] io::Error),

    /// Binding/listening to the given address did not succeed.
    #[cfg(feature = "tokio-exporter")]
    #[error("failed to bind to given listen address: {0}")]
    Hyper(#[from] HyperError),

    /// Installing the recorder did not succeed.
    #[error("failed to install exporter as global recorder: {0}")]
    Recorder(#[from] SetRecorderError),
}

#[derive(Clone)]
enum Distribution {
    /// A Prometheus histogram.
    ///
    /// Exposes "bucketed" values to Prometheus, counting the number of samples
    /// below a given threshold i.e. 100 requests faster than 20ms, 1000 requests
    /// faster than 50ms, etc.
    Histogram(Histogram),
    /// A Prometheus summary.
    ///
    /// Computes and exposes value quantiles directly to Prometheus i.e. 50% of
    /// requests were faster than 200ms, and 99% of requests were faster than
    /// 1000ms, etc.
    Summary(HdrHistogram, u64),
}

struct Snapshot {
    pub counters: HashMap<String, HashMap<Vec<String>, u64>>,
    pub gauges: HashMap<String, HashMap<Vec<String>, f64>>,
    pub distributions: HashMap<String, HashMap<Vec<String>, Distribution>>,
}

pub(crate) struct Inner {
    registry: PrometheusRegistry,
    recency_mask: MetricType,
    recency: Mutex<(Clock, HashMap<CompositeKey, (Generation, Instant)>)>,
    idle_timeout: Option<Duration>,
    distributions: RwLock<HashMap<String, HashMap<Vec<String>, Distribution>>>,
    quantiles: Vec<Quantile>,
    buckets: Vec<u64>,
    buckets_by_name: Option<HashMap<Matcher, Vec<u64>>>,
    descriptions: RwLock<HashMap<String, &'static str>>,
}

impl Inner {
    pub fn registry(&self) -> &PrometheusRegistry {
        &self.registry
    }

    fn should_store(
        &self,
        key: &CompositeKey,
        current_gen: Generation,
        clock: &mut Clock,
        recency: &mut HashMap<CompositeKey, (Generation, Instant)>,
    ) -> bool {
        if let Some(idle_timeout) = self.idle_timeout {
            let now = clock.now();
            if let Some((last_gen, last_update)) = recency.get_mut(&key) {
                // If the value is the same as the latest value we have internally, and
                // we're over the idle timeout period, then remove it and continue.
                if *last_gen == current_gen {
                    if (now - *last_update) > idle_timeout {
                        // If the delete returns false, that means that our generation counter is
                        // out-of-date, and that the metric has been updated since, so we don't
                        // actually want to delete it yet.
                        if self.registry.delete(&key, current_gen) {
                            return false;
                        }
                    }
                } else {
                    // Value has changed, so mark it such.
                    *last_update = now;
                }
            } else {
                recency.insert(key.clone(), (current_gen, now));
            }
        }

        true
    }

    fn get_recent_metrics(&self) -> Snapshot {
        let metrics = self.registry.get_handles();

        let mut rg = self.recency.lock();
        let (clock, recency) = rg.deref_mut();
        let mut counters = HashMap::new();
        let mut gauges = HashMap::new();

        let mut sorted_overrides = self
            .buckets_by_name
            .as_ref()
            .map(|h| Vec::from_iter(h.iter()))
            .unwrap_or_else(|| vec![]);
        sorted_overrides.sort();

        for (key, (gen, handle)) in metrics.into_iter() {
            match key.kind() {
                MetricKind::Counter => {
                    let value = handle.read_counter();
                    if self.recency_mask.has(MetricType::COUNTER) {
                        if !self.should_store(&key, gen, clock, recency) {
                            continue;
                        }
                    }

                    let (_, key) = key.into_parts();
                    let (name, labels) = key_to_parts(key);
                    let entry = counters
                        .entry(name)
                        .or_insert_with(|| HashMap::new())
                        .entry(labels)
                        .or_insert(0);
                    *entry = value;
                }
                MetricKind::Gauge => {
                    let value = handle.read_gauge();
                    if self.recency_mask.has(MetricType::GAUGE) {
                        if !self.should_store(&key, gen, clock, recency) {
                            continue;
                        }
                    }

                    let (_, key) = key.into_parts();
                    let (name, labels) = key_to_parts(key);
                    let entry = gauges
                        .entry(name)
                        .or_insert_with(|| HashMap::new())
                        .entry(labels)
                        .or_insert(0.0);
                    *entry = value;
                }
                MetricKind::Histogram => {
                    if self.recency_mask.has(MetricType::HISTOGRAM) {
                        if !self.should_store(&key, gen, clock, recency) {
                            continue;
                        }
                    }

                    let (_, key) = key.into_parts();
                    let (name, labels) = key_to_parts(key);

                    let buckets = sorted_overrides
                        .iter()
                        .find(|(k, _)| (*k).matches(name.as_str()))
                        .map(|(_, buckets)| *buckets)
                        .unwrap_or(&self.buckets);

                    let mut wg = self.distributions.write();
                    let entry = wg
                        .entry(name.clone())
                        .or_insert_with(|| HashMap::new())
                        .entry(labels)
                        .or_insert_with(|| match buckets.is_empty() {
                            false => {
                                let histogram = Histogram::new(buckets)
                                    .expect("failed to create histogram with buckets defined");
                                Distribution::Histogram(histogram)
                            }
                            true => {
                                let summary =
                                    HdrHistogram::new(3).expect("failed to create histogram");
                                Distribution::Summary(summary, 0)
                            }
                        });

                    match entry {
                        Distribution::Histogram(histogram) => handle
                            .read_histogram_with_clear(|samples| histogram.record_many(samples)),
                        Distribution::Summary(summary, sum) => {
                            handle.read_histogram_with_clear(|samples| {
                                for sample in samples {
                                    let _ = summary.record(*sample);
                                    *sum += *sample;
                                }
                            })
                        }
                    }
                }
            }
        }

        let distributions = self.distributions.read().clone();

        Snapshot {
            counters,
            gauges,
            distributions,
        }
    }

    pub fn render(&self) -> String {
        let mut sorted_overrides = self
            .buckets_by_name
            .as_ref()
            .map(|h| Vec::from_iter(h.iter()))
            .unwrap_or_else(|| vec![]);
        sorted_overrides.sort();

        let Snapshot {
            mut counters,
            mut gauges,
            mut distributions,
        } = self.get_recent_metrics();

        let mut output = String::new();
        let descriptions = self.descriptions.read();

        for (name, mut by_labels) in counters.drain() {
            if let Some(desc) = descriptions.get(name.as_str()) {
                output.push_str("# HELP ");
                output.push_str(name.as_str());
                output.push_str(" ");
                output.push_str(desc);
                output.push_str("\n");
            }

            output.push_str("# TYPE ");
            output.push_str(name.as_str());
            output.push_str(" counter\n");
            for (labels, value) in by_labels.drain() {
                let full_name = render_labeled_name(&name, &labels);
                output.push_str(full_name.as_str());
                output.push_str(" ");
                output.push_str(value.to_string().as_str());
                output.push_str("\n");
            }
            output.push_str("\n");
        }

        for (name, mut by_labels) in gauges.drain() {
            if let Some(desc) = descriptions.get(name.as_str()) {
                output.push_str("# HELP ");
                output.push_str(name.as_str());
                output.push_str(" ");
                output.push_str(desc);
                output.push_str("\n");
            }

            output.push_str("# TYPE ");
            output.push_str(name.as_str());
            output.push_str(" gauge\n");
            for (labels, value) in by_labels.drain() {
                let full_name = render_labeled_name(&name, &labels);
                output.push_str(full_name.as_str());
                output.push_str(" ");
                output.push_str(value.to_string().as_str());
                output.push_str("\n");
            }
            output.push_str("\n");
        }

        for (name, mut by_labels) in distributions.drain() {
            if let Some(desc) = descriptions.get(name.as_str()) {
                output.push_str("# HELP ");
                output.push_str(name.as_str());
                output.push_str(" ");
                output.push_str(desc);
                output.push_str("\n");
            }

            let has_buckets = sorted_overrides
                .iter()
                .any(|(k, _)| !self.buckets.is_empty() || (*k).matches(name.as_str()));

            output.push_str("# TYPE ");
            output.push_str(name.as_str());
            output.push_str(" ");
            output.push_str(if has_buckets { "histogram" } else { "summary" });
            output.push_str("\n");

            for (labels, distribution) in by_labels.drain() {
                let (sum, count) = match distribution {
                    Distribution::Summary(summary, sum) => {
                        for quantile in &self.quantiles {
                            let value = summary.value_at_quantile(quantile.value());
                            let mut labels = labels.clone();
                            labels.push(format!("quantile=\"{}\"", quantile.value()));
                            let full_name = render_labeled_name(&name, &labels);
                            output.push_str(full_name.as_str());
                            output.push_str(" ");
                            output.push_str(value.to_string().as_str());
                            output.push_str("\n");
                        }

                        (sum, summary.len())
                    }
                    Distribution::Histogram(histogram) => {
                        for (le, count) in histogram.buckets() {
                            let mut labels = labels.clone();
                            labels.push(format!("le=\"{}\"", le));
                            let bucket_name = format!("{}_bucket", name);
                            let full_name = render_labeled_name(&bucket_name, &labels);
                            output.push_str(full_name.as_str());
                            output.push_str(" ");
                            output.push_str(count.to_string().as_str());
                            output.push_str("\n");
                        }

                        let mut labels = labels.clone();
                        labels.push("le=\"+Inf\"".to_owned());
                        let bucket_name = format!("{}_bucket", name);
                        let full_name = render_labeled_name(&bucket_name, &labels);
                        output.push_str(full_name.as_str());
                        output.push_str(" ");
                        output.push_str(histogram.count().to_string().as_str());
                        output.push_str("\n");

                        (histogram.sum(), histogram.count())
                    }
                };

                let sum_name = format!("{}_sum", name);
                let full_sum_name = render_labeled_name(&sum_name, &labels);
                output.push_str(full_sum_name.as_str());
                output.push_str(" ");
                output.push_str(sum.to_string().as_str());
                output.push_str("\n");
                let count_name = format!("{}_count", name);
                let full_count_name = render_labeled_name(&count_name, &labels);
                output.push_str(full_count_name.as_str());
                output.push_str(" ");
                output.push_str(count.to_string().as_str());
                output.push_str("\n");
            }

            output.push_str("\n");
        }

        output
    }
}

/// A Prometheus recorder.
///
/// This recorder should be composed with other recorders or installed globally via
/// [`metrics::set_boxed_recorder`].
///
///
pub struct PrometheusRecorder {
    inner: Arc<Inner>,
}

impl PrometheusRecorder {
    /// Gets a [`PrometheusHandle`] to this recorder.
    pub fn handle(&self) -> PrometheusHandle {
        PrometheusHandle {
            inner: self.inner.clone(),
        }
    }

    fn add_description_if_missing(&self, key: &Key, description: Option<&'static str>) {
        if let Some(description) = description {
            let mut descriptions = self.inner.descriptions.write();
            if !descriptions.contains_key(key.name().to_string().as_str()) {
                descriptions.insert(key.name().to_string(), description);
            }
        }
    }
}

/// Handle to [`PrometheusRecorder`].
///
/// Useful for exposing a scrape endpoint on an existing HTTP/HTTPS server.
pub struct PrometheusHandle {
    inner: Arc<Inner>,
}

impl PrometheusHandle {
    /// Returns the metrics in Prometheus accepted String format.
    pub fn render(&self) -> String {
        self.inner.render()
    }
}

/// Builder for creating and installing a Prometheus recorder/exporter.
pub struct PrometheusBuilder {
    listen_address: SocketAddr,
    quantiles: Vec<Quantile>,
    buckets: Vec<u64>,
    idle_timeout: Option<Duration>,
    recency_mask: MetricType,
    buckets_by_name: Option<HashMap<Matcher, Vec<u64>>>,
}

impl PrometheusBuilder {
    /// Creates a new [`PrometheusBuilder`].
    pub fn new() -> Self {
        let quantiles = parse_quantiles(&[0.0, 0.5, 0.9, 0.95, 0.99, 0.999, 1.0]);

        Self {
            listen_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9000),
            quantiles,
            buckets: vec![],
            idle_timeout: None,
            recency_mask: MetricType::NONE,
            buckets_by_name: None,
        }
    }

    /// Sets the listen address for the Prometheus scrape endpoint.
    ///
    /// The HTTP listener that is spawned will respond to GET requests on any request path.
    ///
    /// Defaults to `127.0.0.1:9000`.
    pub fn listen_address(mut self, addr: impl Into<SocketAddr>) -> Self {
        self.listen_address = addr.into();
        self
    }

    /// Sets the quantiles to use when rendering histograms.
    ///
    /// Quantiles represent a scale of 0 to 1, where percentiles represent a scale of 1 to 100, so
    /// a quantile of 0.99 is the 99th percentile, and a quantile of 0.99 is the 99.9th percentile.
    ///
    /// By default, the quantiles will be set to: 0.0, 0.5, 0.9, 0.95, 0.99, 0.999, and 1.0. This means
    /// that all histograms will be exposed as Prometheus summaries.
    ///
    /// If buckets are set (via [`set_buckets`][Self::set_buckets] or
    /// [`set_buckets_for_metric`][Self::set_buckets_for_metric]) then all histograms will be exposed
    /// as summaries instead.
    pub fn set_quantiles(mut self, quantiles: &[f64]) -> Self {
        self.quantiles = parse_quantiles(quantiles);
        self
    }

    /// Sets the buckets to use when rendering histograms.
    ///
    /// Buckets values represent the higher bound of each buckets.  If buckets are set, then all
    /// histograms will be rendered as true Prometheus histograms, instead of summaries.
    pub fn set_buckets(mut self, values: &[u64]) -> Self {
        self.buckets = values.to_vec();
        self
    }

    /// Sets the idle timeout for metrics.
    ///
    /// If a metric hasn't been updated within this timeout, it will be removed from the registry
    /// and in turn removed from the normal scrape output until the metric is emitted again.  This
    /// behavior is driven by requests to generate rendered output, and so metrics will not be
    /// removed unless a request has been made recently enough to prune the idle metrics.
    ///
    /// Further, the metric type "mask" configures which metrics will be considered by the idle
    /// timeout.  If the type of a metric being considered for idle timeout is not of a type
    /// represented by the mask, it will not be affected, even if it would have othered been removed
    /// for exceeding the idle timeout.
    ///
    /// [`MetricType`] can be combined in a bitflags-style approach using the bitwise OR operator,
    /// as such:
    /// ```rust
    /// # use metrics_exporter_prometheus::MetricType;
    /// # fn main() {
    /// let mask = MetricType::COUNTER | MetricType::HISTOGRAM;
    /// # }
    /// ```
    pub fn idle_timeout(mut self, timeout: Option<Duration>, mask: MetricType) -> Self {
        self.idle_timeout = timeout;
        self.recency_mask = if self.idle_timeout.is_none() {
            MetricType::NONE
        } else {
            mask
        };
        self
    }

    /// Sets the bucket for a specific pattern.
    ///
    /// The match pattern can be a full match (equality), prefix match, or suffix match.  The
    /// matchers are applied in that order if two or more matchers would apply to a single metric.
    /// That is to say, if a full match and a prefix match applied to a metric, the full match would
    /// win, and if a prefix match and a suffix match applied to a metric, the prefix match would win.
    ///
    /// Buckets values represent the higher bound of each buckets.  If buckets are set, then any
    /// histograms that match will be rendered as true Prometheus histograms, instead of summaries.
    ///
    /// This option changes the observer's output of histogram-type metric into summaries.
    /// It only affects matching metrics if set_buckets was not used.
    pub fn set_buckets_for_metric(mut self, matcher: Matcher, values: &[u64]) -> Self {
        let buckets = self.buckets_by_name.get_or_insert_with(|| HashMap::new());
        buckets.insert(matcher, values.to_vec());
        self
    }

    /// Builds the recorder and exporter and installs them globally.
    ///
    /// An error will be returned if there's an issue with creating the HTTP server or with
    /// installing the recorder as the global recorder.
    #[cfg(feature = "tokio-exporter")]
    pub fn install(self) -> Result<(), Error> {
        let mut runtime = runtime::Builder::new()
            .basic_scheduler()
            .enable_all()
            .build()?;

        let (recorder, exporter) = runtime.enter(|| self.build_with_exporter())?;
        metrics::set_boxed_recorder(Box::new(recorder))?;

        thread::Builder::new()
            .name("metrics-exporter-prometheus-http".to_string())
            .spawn(move || {
                runtime.block_on(async move {
                    pin!(exporter);
                    loop {
                        select! {
                            _ = &mut exporter => {}
                        }
                    }
                });
            })?;

        Ok(())
    }

    /// Builds the recorder and returns it.
    /// This function is only enabled when default features are not set.
    pub fn build(self) -> Result<PrometheusRecorder, Error> {
        self.build_with_clock(Clock::new())
    }

    pub(crate) fn build_with_clock(self, clock: Clock) -> Result<PrometheusRecorder, Error> {
        let inner = Arc::new(Inner {
            registry: Registry::new(),
            recency: Mutex::new((clock, HashMap::new())),
            idle_timeout: self.idle_timeout,
            recency_mask: self.recency_mask,
            distributions: RwLock::new(HashMap::new()),
            quantiles: self.quantiles.clone(),
            buckets: self.buckets.clone(),
            buckets_by_name: self.buckets_by_name,
            descriptions: RwLock::new(HashMap::new()),
        });

        let recorder = PrometheusRecorder { inner };

        Ok(recorder)
    }

    /// Builds the recorder and exporter and returns them both.
    ///
    /// In most cases, users should prefer to use [`PrometheusBuilder::install`] to create and
    /// install the recorder and exporter automatically for them.  If a caller is combining
    /// recorders, or needs to schedule the exporter to run in a particular way, this method
    /// provides the flexibility to do so.
    #[cfg(feature = "tokio-exporter")]
    pub fn build_with_exporter(
        self,
    ) -> Result<
        (
            PrometheusRecorder,
            impl Future<Output = Result<(), HyperError>> + Send + 'static,
        ),
        Error,
    > {
        let inner = Arc::new(Inner {
            registry: Registry::new(),
            recency: Mutex::new((Clock::new(), HashMap::new())),
            idle_timeout: self.idle_timeout,
            recency_mask: self.recency_mask,
            distributions: RwLock::new(HashMap::new()),
            quantiles: self.quantiles.clone(),
            buckets: self.buckets.clone(),
            buckets_by_name: self.buckets_by_name.clone(),
            descriptions: RwLock::new(HashMap::new()),
        });

        let recorder = PrometheusRecorder {
            inner: inner.clone(),
        };

        let address = self.listen_address;
        let server = Server::try_bind(&address)?;

        let exporter = async move {
            let make_svc = make_service_fn(move |_| {
                let inner = inner.clone();

                async move {
                    Ok::<_, HyperError>(service_fn(move |_| {
                        let inner = inner.clone();

                        async move {
                            let output = inner.render();
                            Ok::<_, HyperError>(Response::new(Body::from(output)))
                        }
                    }))
                }
            });

            server.serve(make_svc).await
        };

        Ok((recorder, exporter))
    }
}

impl Recorder for PrometheusRecorder {
    fn register_counter(&self, key: Key, _unit: Option<Unit>, description: Option<&'static str>) {
        self.add_description_if_missing(&key, description);
        self.inner.registry().op(
            CompositeKey::new(MetricKind::Counter, key),
            |_| {},
            || Handle::counter(),
        );
    }

    fn register_gauge(&self, key: Key, _unit: Option<Unit>, description: Option<&'static str>) {
        self.add_description_if_missing(&key, description);
        self.inner.registry().op(
            CompositeKey::new(MetricKind::Gauge, key),
            |_| {},
            || Handle::gauge(),
        );
    }

    fn register_histogram(&self, key: Key, _unit: Option<Unit>, description: Option<&'static str>) {
        self.add_description_if_missing(&key, description);
        self.inner.registry().op(
            CompositeKey::new(MetricKind::Histogram, key),
            |_| {},
            || Handle::histogram(),
        );
    }

    fn increment_counter(&self, key: Key, value: u64) {
        self.inner.registry().op(
            CompositeKey::new(MetricKind::Counter, key),
            |h| h.increment_counter(value),
            || Handle::counter(),
        );
    }

    fn update_gauge(&self, key: Key, value: f64) {
        self.inner.registry().op(
            CompositeKey::new(MetricKind::Gauge, key),
            |h| h.update_gauge(value),
            || Handle::gauge(),
        );
    }

    fn record_histogram(&self, key: Key, value: u64) {
        self.inner.registry().op(
            CompositeKey::new(MetricKind::Histogram, key),
            |h| h.record_histogram(value),
            || Handle::histogram(),
        );
    }
}

fn key_to_parts(key: Key) -> (String, Vec<String>) {
    let sanitize = |c| c == '.' || c == '=' || c == '{' || c == '}' || c == '+' || c == '-';
    let name = key.name().to_string().replace(sanitize, "_");
    let labels = key
        .labels()
        .into_iter()
        .map(|label| {
            let k = label.key();
            let v = label.value();
            format!(
                "{}=\"{}\"",
                k,
                v.replace("\\", "\\\\")
                    .replace("\"", "\\\"")
                    .replace("\n", "\\n")
            )
        })
        .collect();

    (name, labels)
}

fn render_labeled_name(name: &str, labels: &[String]) -> String {
    let mut output = name.to_string();
    if !labels.is_empty() {
        let joined = labels.join(",");
        output.push_str("{");
        output.push_str(&joined);
        output.push_str("}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{Matcher, MetricType, PrometheusBuilder};
    use metrics::{Key, KeyData, Recorder};
    use quanta::Clock;
    use std::time::Duration;

    #[test]
    fn test_creation() {
        let recorder = PrometheusBuilder::new().build();
        assert!(recorder.is_ok());
    }

    #[test]
    fn test_render() {
        let recorder = PrometheusBuilder::new()
            .build()
            .expect("failed to create PrometheusRecorder");

        let key = Key::from(KeyData::from_name("basic_counter"));
        recorder.increment_counter(key, 42);

        let handle = recorder.handle();
        let rendered = handle.render();
        let expected_counter = "# TYPE basic_counter counter\nbasic_counter 42\n\n";

        assert_eq!(rendered, expected_counter);

        let key = Key::from(KeyData::from_name("basic_gauge"));
        recorder.update_gauge(key, -3.14);
        let rendered = handle.render();
        let expected_gauge = format!(
            "{}# TYPE basic_gauge gauge\nbasic_gauge -3.14\n\n",
            expected_counter
        );

        assert_eq!(rendered, expected_gauge);

        let key = Key::from(KeyData::from_name("basic_histogram"));
        recorder.record_histogram(key, 12);
        let rendered = handle.render();

        let histogram_data = concat!(
            "# TYPE basic_histogram summary\n",
            "basic_histogram{quantile=\"0\"} 12\n",
            "basic_histogram{quantile=\"0.5\"} 12\n",
            "basic_histogram{quantile=\"0.9\"} 12\n",
            "basic_histogram{quantile=\"0.95\"} 12\n",
            "basic_histogram{quantile=\"0.99\"} 12\n",
            "basic_histogram{quantile=\"0.999\"} 12\n",
            "basic_histogram{quantile=\"1\"} 12\n",
            "basic_histogram_sum 12\n",
            "basic_histogram_count 1\n",
            "\n"
        );
        let expected_histogram = format!("{}{}", expected_gauge, histogram_data);

        assert_eq!(rendered, expected_histogram);
    }

    #[test]
    fn test_buckets() {
        const DEFAULT_VALUES: [u64; 3] = [10, 100, 1000];
        const PREFIX_VALUES: [u64; 3] = [15, 105, 1005];
        const SUFFIX_VALUES: [u64; 3] = [20, 110, 1010];
        const FULL_VALUES: [u64; 3] = [25, 115, 1015];

        let recorder = PrometheusBuilder::new()
            .set_buckets_for_metric(
                Matcher::Full("metrics_testing_foo".to_owned()),
                &FULL_VALUES[..],
            )
            .set_buckets_for_metric(
                Matcher::Prefix("metrics_testing".to_owned()),
                &PREFIX_VALUES[..],
            )
            .set_buckets_for_metric(Matcher::Suffix("foo".to_owned()), &SUFFIX_VALUES[..])
            .set_buckets(&DEFAULT_VALUES[..])
            .build()
            .expect("failed to create PrometheusRecorder");

        let full_key = Key::from(KeyData::from_name("metrics_testing_foo"));
        recorder.record_histogram(full_key, FULL_VALUES[0]);

        let prefix_key = Key::from(KeyData::from_name("metrics_testing_bar"));
        recorder.record_histogram(prefix_key, PREFIX_VALUES[1]);

        let suffix_key = Key::from(KeyData::from_name("metrics_testin_foo"));
        recorder.record_histogram(suffix_key, SUFFIX_VALUES[2]);

        let default_key = Key::from(KeyData::from_name("metrics_wee"));
        recorder.record_histogram(default_key, DEFAULT_VALUES[2] + 1);

        let full_data = concat!(
            "# TYPE metrics_testing_foo histogram\n",
            "metrics_testing_foo_bucket{le=\"25\"} 1\n",
            "metrics_testing_foo_bucket{le=\"115\"} 1\n",
            "metrics_testing_foo_bucket{le=\"1015\"} 1\n",
            "metrics_testing_foo_bucket{le=\"+Inf\"} 1\n",
            "metrics_testing_foo_sum 25\n",
            "metrics_testing_foo_count 1\n",
        );

        let prefix_data = concat!(
            "# TYPE metrics_testing_bar histogram\n",
            "metrics_testing_bar_bucket{le=\"15\"} 0\n",
            "metrics_testing_bar_bucket{le=\"105\"} 1\n",
            "metrics_testing_bar_bucket{le=\"1005\"} 1\n",
            "metrics_testing_bar_bucket{le=\"+Inf\"} 1\n",
            "metrics_testing_bar_sum 105\n",
            "metrics_testing_bar_count 1\n",
        );

        let suffix_data = concat!(
            "# TYPE metrics_testin_foo histogram\n",
            "metrics_testin_foo_bucket{le=\"20\"} 0\n",
            "metrics_testin_foo_bucket{le=\"110\"} 0\n",
            "metrics_testin_foo_bucket{le=\"1010\"} 1\n",
            "metrics_testin_foo_bucket{le=\"+Inf\"} 1\n",
            "metrics_testin_foo_sum 1010\n",
            "metrics_testin_foo_count 1\n",
        );

        let default_data = concat!(
            "# TYPE metrics_wee histogram\n",
            "metrics_wee_bucket{le=\"10\"} 0\n",
            "metrics_wee_bucket{le=\"100\"} 0\n",
            "metrics_wee_bucket{le=\"1000\"} 0\n",
            "metrics_wee_bucket{le=\"+Inf\"} 1\n",
            "metrics_wee_sum 1001\n",
            "metrics_wee_count 1\n",
        );

        let handle = recorder.handle();
        let rendered = handle.render();

        assert!(rendered.contains(full_data));
        assert!(rendered.contains(prefix_data));
        assert!(rendered.contains(suffix_data));
        assert!(rendered.contains(default_data));
    }

    #[test]
    fn test_idle_timeout() {
        let (clock, mock) = Clock::mock();

        let recorder = PrometheusBuilder::new()
            .idle_timeout(Some(Duration::from_secs(10)), MetricType::COUNTER)
            .build_with_clock(clock)
            .expect("failed to create PrometheusRecorder");

        let key = Key::from(KeyData::from_name("basic_counter"));
        recorder.increment_counter(key, 42);

        let key = Key::from(KeyData::from_name("basic_gauge"));
        recorder.update_gauge(key, -3.14);

        let handle = recorder.handle();
        let rendered = handle.render();
        let expected = concat!(
            "# TYPE basic_counter counter\n",
            "basic_counter 42\n\n",
            "# TYPE basic_gauge gauge\n",
            "basic_gauge -3.14\n\n",
        );

        assert_eq!(rendered, expected);

        mock.increment(Duration::from_secs(9));
        let rendered = handle.render();
        assert_eq!(rendered, expected);

        mock.increment(Duration::from_secs(2));
        let rendered = handle.render();

        let expected = "# TYPE basic_gauge gauge\nbasic_gauge -3.14\n\n";
        assert_eq!(rendered, expected);
    }
}
