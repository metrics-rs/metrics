use std::collections::HashMap;
#[cfg(feature = "push-gateway")]
use std::convert::TryFrom;
#[cfg(feature = "http-listener")]
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::RwLock;
#[cfg(any(feature = "http-listener", feature = "push-gateway"))]
use std::thread;
use std::time::Duration;

#[cfg(feature = "push-gateway")]
use hyper::Uri;
use indexmap::IndexMap;
#[cfg(feature = "http-listener")]
use ipnet::IpNet;
use quanta::Clock;

use metrics_util::{
    parse_quantiles,
    registry::{GenerationalStorage, Recency, Registry},
    MetricKindMask, Quantile,
};

use crate::common::Matcher;
use crate::distribution::DistributionBuilder;
use crate::recorder::{Inner, PrometheusRecorder};
use crate::registry::AtomicStorage;
use crate::{common::BuildError, PrometheusHandle};

use super::ExporterConfig;
#[cfg(any(feature = "http-listener", feature = "push-gateway"))]
use super::ExporterFuture;

/// Builder for creating and installing a Prometheus recorder/exporter.
pub struct PrometheusBuilder {
    #[cfg_attr(not(any(feature = "http-listener", feature = "push-gateway")), allow(dead_code))]
    exporter_config: ExporterConfig,
    #[cfg(feature = "http-listener")]
    allowed_addresses: Option<Vec<IpNet>>,
    quantiles: Vec<Quantile>,
    bucket_duration: Option<Duration>,
    bucket_count: Option<NonZeroU32>,
    buckets: Option<Vec<f64>>,
    bucket_overrides: Option<HashMap<Matcher, Vec<f64>>>,
    idle_timeout: Option<Duration>,
    upkeep_timeout: Duration,
    recency_mask: MetricKindMask,
    global_labels: Option<IndexMap<String, String>>,
}

impl PrometheusBuilder {
    /// Creates a new [`PrometheusBuilder`].
    pub fn new() -> Self {
        let quantiles = parse_quantiles(&[0.0, 0.5, 0.9, 0.95, 0.99, 0.999, 1.0]);

        #[cfg(feature = "http-listener")]
        let exporter_config = ExporterConfig::HttpListener {
            destination: super::ListenDestination::Tcp(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
                9000,
            )),
        };
        #[cfg(not(feature = "http-listener"))]
        let exporter_config = ExporterConfig::Unconfigured;

        let upkeep_timeout = Duration::from_secs(5);

        Self {
            exporter_config,
            #[cfg(feature = "http-listener")]
            allowed_addresses: None,
            quantiles,
            bucket_duration: None,
            bucket_count: None,
            buckets: None,
            bucket_overrides: None,
            idle_timeout: None,
            upkeep_timeout,
            recency_mask: MetricKindMask::NONE,
            global_labels: None,
        }
    }

    /// Configures the exporter to expose an HTTP listener that functions as a [scrape endpoint].
    ///
    /// The HTTP listener that is spawned will respond to GET requests on any request path.
    ///
    /// Running in HTTP listener mode is mutually exclusive with the push gateway i.e. enabling the
    /// HTTP listener will disable the push gateway, and vise versa.
    ///
    /// Defaults to enabled, listening at `0.0.0.0:9000`.
    ///
    /// [scrape endpoint]: https://prometheus.io/docs/instrumenting/exposition_formats/#text-based-format
    #[cfg(feature = "http-listener")]
    #[cfg_attr(docsrs, doc(cfg(feature = "http-listener")))]
    #[must_use]
    pub fn with_http_listener(mut self, addr: impl Into<SocketAddr>) -> Self {
        self.exporter_config = ExporterConfig::HttpListener {
            destination: super::ListenDestination::Tcp(addr.into()),
        };
        self
    }

    /// Configures the exporter to push periodic requests to a Prometheus [push gateway].
    ///
    /// Running in push gateway mode is mutually exclusive with the HTTP listener i.e. enabling the
    /// push gateway will disable the HTTP listener, and vise versa.
    ///
    /// Defaults to disabled.
    ///
    /// ## Errors
    ///
    /// If the given endpoint cannot be parsed into a valid URI, an error variant will be
    /// returned describing the error.
    ///
    /// [push gateway]: https://prometheus.io/docs/instrumenting/pushing/
    #[cfg(feature = "push-gateway")]
    #[cfg_attr(docsrs, doc(cfg(feature = "push-gateway")))]
    pub fn with_push_gateway<T>(
        mut self,
        endpoint: T,
        interval: Duration,
        username: Option<String>,
        password: Option<String>,
    ) -> Result<Self, BuildError>
    where
        T: AsRef<str>,
    {
        self.exporter_config = ExporterConfig::PushGateway {
            endpoint: Uri::try_from(endpoint.as_ref())
                .map_err(|e| BuildError::InvalidPushGatewayEndpoint(e.to_string()))?,
            interval,
            username,
            password,
        };

        Ok(self)
    }

    /// Configures the exporter to expose an HTTP listener that functions as a [scrape endpoint],
    /// listening on a Unix Domain socket at the given path
    ///
    /// The HTTP listener that is spawned will respond to GET requests on any request path.
    ///
    /// Running in HTTP listener mode is mutually exclusive with the push gateway i.e. enabling the
    /// HTTP listener will disable the push gateway, and vise versa.
    ///
    /// Defaults to disabled.
    ///
    /// [scrape endpoint]: https://prometheus.io/docs/instrumenting/exposition_formats/#text-based-format
    #[cfg(feature = "uds-listener")]
    #[cfg_attr(docsrs, doc(cfg(feature = "uds-listener")))]
    #[must_use]
    pub fn with_http_uds_listener(mut self, addr: impl Into<std::path::PathBuf>) -> Self {
        self.exporter_config = ExporterConfig::HttpListener {
            destination: super::ListenDestination::Uds(addr.into()),
        };
        self
    }

    /// Adds an IP address or subnet to the allowlist for the scrape endpoint.
    ///
    /// If a client makes a request to the scrape endpoint and their IP is not present in the
    /// allowlist, either directly or within any of the allowed subnets, they will receive a 403
    /// Forbidden response.
    ///
    /// Defaults to allowing all IPs.
    ///
    /// ## Security Considerations
    ///
    /// On its own, an IP allowlist is insufficient for access control, if the exporter is running
    /// in an environment alongside applications (such as web browsers) that are susceptible to [DNS
    /// rebinding](https://en.wikipedia.org/wiki/DNS_rebinding) attacks.
    ///
    /// ## Errors
    ///
    /// If the given address cannot be parsed into an IP address or subnet, an error variant will be
    /// returned describing the error.
    #[cfg(feature = "http-listener")]
    #[cfg_attr(docsrs, doc(cfg(feature = "http-listener")))]
    pub fn add_allowed_address<A>(mut self, address: A) -> Result<Self, BuildError>
    where
        A: AsRef<str>,
    {
        use std::str::FromStr;

        let address = IpNet::from_str(address.as_ref())
            .map_err(|e| BuildError::InvalidAllowlistAddress(e.to_string()))?;
        self.allowed_addresses.get_or_insert(vec![]).push(address);

        Ok(self)
    }

    /// Sets the quantiles to use when rendering histograms.
    ///
    /// Quantiles represent a scale of 0 to 1, where percentiles represent a scale of 1 to 100, so
    /// a quantile of 0.99 is the 99th percentile, and a quantile of 0.99 is the 99.9th percentile.
    ///
    /// Defaults to a hard-coded set of quantiles: 0.0, 0.5, 0.9, 0.95, 0.99, 0.999, and 1.0. This means
    /// that all histograms will be exposed as Prometheus summaries.
    ///
    /// If buckets are set (via [`set_buckets`][Self::set_buckets] or
    /// [`set_buckets_for_metric`][Self::set_buckets_for_metric]) then all histograms will be exposed
    /// as summaries instead.
    ///
    /// ## Errors
    ///
    /// If `quantiles` is empty, an error variant will be thrown.
    pub fn set_quantiles(mut self, quantiles: &[f64]) -> Result<Self, BuildError> {
        if quantiles.is_empty() {
            return Err(BuildError::EmptyBucketsOrQuantiles);
        }

        self.quantiles = parse_quantiles(quantiles);
        Ok(self)
    }

    /// Sets the bucket width when using summaries.
    ///
    /// Summaries are rolling, which means that they are divided into buckets of a fixed duration
    /// (width), and older buckets are dropped as they age out. This means data from a period as
    /// large as the width will be dropped at a time.
    ///
    /// The total amount of data kept for a summary is the number of buckets times the bucket width.
    /// For example, a bucket count of 3 and a bucket width of 20 seconds would mean that 60 seconds
    /// of data is kept at most, with the oldest 20 second chunk of data being dropped as the
    /// summary rolls forward.
    ///
    /// Use more buckets with a smaller width to roll off smaller amounts of data at a time, or
    /// fewer buckets with a larger width to roll it off in larger chunks.
    ///
    /// Defaults to 20 seconds.
    ///
    /// ## Errors
    ///
    /// If the duration given is zero, an error variant will be thrown.
    pub fn set_bucket_duration(mut self, value: Duration) -> Result<Self, BuildError> {
        if value.is_zero() {
            return Err(BuildError::ZeroBucketDuration);
        }

        self.bucket_duration = Some(value);
        Ok(self)
    }

    /// Sets the bucket count when using summaries.
    ///
    /// Summaries are rolling, which means that they are divided into buckets of a fixed duration
    /// (width), and older buckets are dropped as they age out. This means data from a period as
    /// large as the width will be dropped at a time.
    ///
    /// The total amount of data kept for a summary is the number of buckets times the bucket width.
    /// For example, a bucket count of 3 and a bucket width of 20 seconds would mean that 60 seconds
    /// of data is kept at most, with the oldest 20 second chunk of data being dropped as the
    /// summary rolls forward.
    ///
    /// Use more buckets with a smaller width to roll off smaller amounts of data at a time, or
    /// fewer buckets with a larger width to roll it off in larger chunks.
    ///
    /// Defaults to 3.
    #[must_use]
    pub fn set_bucket_count(mut self, count: NonZeroU32) -> Self {
        self.bucket_count = Some(count);
        self
    }

    /// Sets the buckets to use when rendering histograms.
    ///
    /// Buckets values represent the higher bound of each buckets.  If buckets are set, then all
    /// histograms will be rendered as true Prometheus histograms, instead of summaries.
    ///
    /// ## Errors
    ///
    /// If `values` is empty, an error variant will be thrown.
    pub fn set_buckets(mut self, values: &[f64]) -> Result<Self, BuildError> {
        if values.is_empty() {
            return Err(BuildError::EmptyBucketsOrQuantiles);
        }

        self.buckets = Some(values.to_vec());
        Ok(self)
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
    /// It only affects matching metrics if [`set_buckets`][Self::set_buckets] was not used.
    ///
    /// ## Errors
    ///
    /// If `values` is empty, an error variant will be thrown.
    pub fn set_buckets_for_metric(
        mut self,
        matcher: Matcher,
        values: &[f64],
    ) -> Result<Self, BuildError> {
        if values.is_empty() {
            return Err(BuildError::EmptyBucketsOrQuantiles);
        }

        let buckets = self.bucket_overrides.get_or_insert_with(HashMap::new);
        buckets.insert(matcher.sanitized(), values.to_vec());
        Ok(self)
    }

    /// Sets the idle timeout for metrics.
    ///
    /// If a metric hasn't been updated within this timeout, it will be removed from the registry
    /// and in turn removed from the normal scrape output until the metric is emitted again.  This
    /// behavior is driven by requests to generate rendered output, and so metrics will not be
    /// removed unless a request has been made recently enough to prune the idle metrics.
    ///
    /// Further, the metric kind "mask" configures which metrics will be considered by the idle
    /// timeout.  If the kind of a metric being considered for idle timeout is not of a kind
    /// represented by the mask, it will not be affected, even if it would have othered been removed
    /// for exceeding the idle timeout.
    ///
    /// Refer to the documentation for [`MetricKindMask`](metrics_util::MetricKindMask) for more
    /// information on defining a metric kind mask.
    #[must_use]
    pub fn idle_timeout(mut self, mask: MetricKindMask, timeout: Option<Duration>) -> Self {
        self.idle_timeout = timeout;
        self.recency_mask = if self.idle_timeout.is_none() { MetricKindMask::NONE } else { mask };
        self
    }

    /// Sets the upkeep interval.
    ///
    /// The upkeep task handles periodic maintenance operations, such as draining histogram data,
    /// to ensure that all recorded data is up-to-date and prevent unbounded memory growth.
    #[must_use]
    pub fn upkeep_timeout(mut self, timeout: Duration) -> Self {
        self.upkeep_timeout = timeout;
        self
    }

    /// Adds a global label to this exporter.
    ///
    /// Global labels are applied to all metrics.  Labels defined on the metric key itself have precedence
    /// over any global labels.  If this method is called multiple times, the latest value for a given label
    /// key will be used.
    #[must_use]
    pub fn add_global_label<K, V>(mut self, key: K, value: V) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        let labels = self.global_labels.get_or_insert_with(IndexMap::new);
        labels.insert(key.into(), value.into());
        self
    }

    /// Builds the recorder and exporter and installs them globally.
    ///
    /// When called from within a Tokio runtime, the exporter future is spawned directly
    /// into the runtime.  Otherwise, a new single-threaded Tokio runtime is created
    /// on a background thread, and the exporter is spawned there.
    ///
    /// ## Errors
    ///
    /// If there is an error while either building the recorder and exporter, or installing the
    /// recorder and exporter, an error variant will be returned describing the error.
    #[cfg(any(feature = "http-listener", feature = "push-gateway"))]
    #[cfg_attr(docsrs, doc(cfg(any(feature = "http-listener", feature = "push-gateway"))))]
    pub fn install(self) -> Result<(), BuildError> {
        use tokio::runtime;

        let recorder = if let Ok(handle) = runtime::Handle::try_current() {
            let (recorder, exporter) = {
                let _g = handle.enter();
                self.build()?
            };

            handle.spawn(exporter);

            recorder
        } else {
            let thread_name =
                format!("metrics-exporter-prometheus-{}", self.exporter_config.as_type_str());

            let runtime = runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| BuildError::FailedToCreateRuntime(e.to_string()))?;

            let (recorder, exporter) = {
                let _g = runtime.enter();
                self.build()?
            };

            thread::Builder::new()
                .name(thread_name)
                .spawn(move || runtime.block_on(exporter))
                .map_err(|e| BuildError::FailedToCreateRuntime(e.to_string()))?;

            recorder
        };

        metrics::set_global_recorder(recorder)?;

        Ok(())
    }

    /// Builds the recorder and installs it globally, returning a handle to it.
    ///
    /// The handle can be used to generate valid Prometheus scrape endpoint payloads directly.
    ///
    /// ## Errors
    ///
    /// If there is an error while building the recorder, or installing the recorder, an error
    /// variant will be returned describing the error.
    pub fn install_recorder(self) -> Result<PrometheusHandle, BuildError> {
        let recorder = self.build_recorder();
        let handle = recorder.handle();

        metrics::set_global_recorder(recorder)?;

        Ok(handle)
    }

    /// Builds the recorder and exporter and returns them both.
    ///
    /// In most cases, users should prefer to use [`install`][PrometheusBuilder::install] to create
    /// and install the recorder and exporter automatically for them.  If a caller is combining
    /// recorders, or needs to schedule the exporter to run in a particular way, this method, or
    /// [`build_recorder`][PrometheusBuilder::build_recorder], provide the flexibility to do so.
    ///
    /// ## Panics
    ///
    /// This method must be called from within an existing Tokio runtime or it will panic.
    ///
    /// ## Errors
    ///
    /// If there is an error while building the recorder and exporter, an error variant will be
    /// returned describing the error.
    #[warn(clippy::too_many_lines)]
    #[cfg(any(feature = "http-listener", feature = "push-gateway"))]
    #[cfg_attr(docsrs, doc(cfg(any(feature = "http-listener", feature = "push-gateway"))))]
    #[cfg_attr(not(feature = "http-listener"), allow(unused_mut))]
    pub fn build(mut self) -> Result<(PrometheusRecorder, ExporterFuture), BuildError> {
        #[cfg(feature = "http-listener")]
        let allowed_addresses = self.allowed_addresses.take();
        let exporter_config = self.exporter_config.clone();
        let upkeep_timeout = self.upkeep_timeout;

        let recorder = self.build_recorder();
        let handle = recorder.handle();

        let recorder_handle = handle.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(upkeep_timeout).await;
                recorder_handle.run_upkeep();
            }
        });

        Ok((
            recorder,
            match exporter_config {
                ExporterConfig::Unconfigured => Err(BuildError::MissingExporterConfiguration)?,

                #[cfg(feature = "http-listener")]
                ExporterConfig::HttpListener { destination } => match destination {
                    super::ListenDestination::Tcp(listen_address) => {
                        super::http_listener::new_http_listener(
                            handle,
                            listen_address,
                            allowed_addresses,
                        )?
                    }
                    #[cfg(feature = "uds-listener")]
                    super::ListenDestination::Uds(listen_path) => {
                        super::http_listener::new_http_uds_listener(handle, listen_path)?
                    }
                },

                #[cfg(feature = "push-gateway")]
                ExporterConfig::PushGateway { endpoint, interval, username, password } => {
                    super::push_gateway::new_push_gateway(
                        endpoint, interval, username, password, handle,
                    )
                }
            },
        ))
    }

    /// Builds the recorder and returns it.
    pub fn build_recorder(self) -> PrometheusRecorder {
        self.build_with_clock(Clock::new())
    }

    pub(crate) fn build_with_clock(self, clock: Clock) -> PrometheusRecorder {
        let inner = Inner {
            registry: Registry::new(GenerationalStorage::new(AtomicStorage)),
            recency: Recency::new(clock, self.recency_mask, self.idle_timeout),
            distributions: RwLock::new(HashMap::new()),
            distribution_builder: DistributionBuilder::new(
                self.quantiles,
                self.bucket_duration,
                self.buckets,
                self.bucket_count,
                self.bucket_overrides,
            ),
            descriptions: RwLock::new(HashMap::new()),
            global_labels: self.global_labels.unwrap_or_default(),
        };

        PrometheusRecorder::from(inner)
    }
}

impl Default for PrometheusBuilder {
    fn default() -> Self {
        PrometheusBuilder::new()
    }
}

#[cfg(test)]
#[allow(clippy::approx_constant)]
mod tests {
    use std::time::Duration;

    use quanta::Clock;

    use metrics::{Key, KeyName, Label, Recorder};
    use metrics_util::MetricKindMask;

    use super::{Matcher, PrometheusBuilder};

    static METADATA: metrics::Metadata =
        metrics::Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));

    #[test]
    fn test_render() {
        let recorder =
            PrometheusBuilder::new().set_quantiles(&[0.0, 1.0]).unwrap().build_recorder();

        let key = Key::from_name("basic_counter");
        let counter1 = recorder.register_counter(&key, &METADATA);
        counter1.increment(42);

        let handle = recorder.handle();
        let rendered = handle.render();
        let expected_counter = "# TYPE basic_counter counter\nbasic_counter 42\n\n";

        assert_eq!(rendered, expected_counter);

        let labels = vec![Label::new("wutang", "forever")];
        let key = Key::from_parts("basic_gauge", labels);
        let gauge1 = recorder.register_gauge(&key, &METADATA);
        gauge1.set(-3.14);
        let rendered = handle.render();
        let expected_gauge = format!(
            "{expected_counter}# TYPE basic_gauge gauge\nbasic_gauge{{wutang=\"forever\"}} -3.14\n\n",
        );

        assert_eq!(rendered, expected_gauge);

        let key = Key::from_name("basic_histogram");
        let histogram1 = recorder.register_histogram(&key, &METADATA);
        histogram1.record(12.0);
        let rendered = handle.render();

        let histogram_data = concat!(
            "# TYPE basic_histogram summary\n",
            "basic_histogram{quantile=\"0\"} 12\n",
            "basic_histogram{quantile=\"1\"} 12\n",
            "basic_histogram_sum 12\n",
            "basic_histogram_count 1\n",
            "\n"
        );
        let expected_histogram = format!("{expected_gauge}{histogram_data}");

        assert_eq!(rendered, expected_histogram);
    }

    #[test]
    fn test_buckets() {
        const DEFAULT_VALUES: [f64; 3] = [10.0, 100.0, 1000.0];
        const PREFIX_VALUES: [f64; 3] = [15.0, 105.0, 1005.0];
        const SUFFIX_VALUES: [f64; 3] = [20.0, 110.0, 1010.0];
        const FULL_VALUES: [f64; 3] = [25.0, 115.0, 1015.0];

        let recorder = PrometheusBuilder::new()
            .set_buckets_for_metric(
                Matcher::Full("metrics.testing foo".to_owned()),
                &FULL_VALUES[..],
            )
            .expect("bounds should not be empty")
            .set_buckets_for_metric(
                Matcher::Prefix("metrics.testing".to_owned()),
                &PREFIX_VALUES[..],
            )
            .expect("bounds should not be empty")
            .set_buckets_for_metric(Matcher::Suffix("foo".to_owned()), &SUFFIX_VALUES[..])
            .expect("bounds should not be empty")
            .set_buckets(&DEFAULT_VALUES[..])
            .expect("bounds should not be empty")
            .build_recorder();

        let full_key = Key::from_name("metrics.testing_foo");
        let full_key_histo = recorder.register_histogram(&full_key, &METADATA);
        full_key_histo.record(FULL_VALUES[0]);

        let prefix_key = Key::from_name("metrics.testing_bar");
        let prefix_key_histo = recorder.register_histogram(&prefix_key, &METADATA);
        prefix_key_histo.record(PREFIX_VALUES[1]);

        let suffix_key = Key::from_name("metrics_testin_foo");
        let suffix_key_histo = recorder.register_histogram(&suffix_key, &METADATA);
        suffix_key_histo.record(SUFFIX_VALUES[2]);

        let default_key = Key::from_name("metrics.wee");
        let default_key_histo = recorder.register_histogram(&default_key, &METADATA);
        default_key_histo.record(DEFAULT_VALUES[2] + 1.0);

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
    fn test_idle_timeout_all() {
        let (clock, mock) = Clock::mock();

        let recorder = PrometheusBuilder::new()
            .idle_timeout(MetricKindMask::ALL, Some(Duration::from_secs(10)))
            .set_quantiles(&[0.0, 1.0])
            .unwrap()
            .build_with_clock(clock);

        let key = Key::from_name("basic_counter");
        let counter1 = recorder.register_counter(&key, &METADATA);
        counter1.increment(42);

        let key = Key::from_name("basic_gauge");
        let gauge1 = recorder.register_gauge(&key, &METADATA);
        gauge1.set(-3.14);

        let key = Key::from_name("basic_histogram");
        let histo1 = recorder.register_histogram(&key, &METADATA);
        histo1.record(1.0);

        let handle = recorder.handle();
        let rendered = handle.render();
        let expected = concat!(
            "# TYPE basic_counter counter\n",
            "basic_counter 42\n\n",
            "# TYPE basic_gauge gauge\n",
            "basic_gauge -3.14\n\n",
            "# TYPE basic_histogram summary\n",
            "basic_histogram{quantile=\"0\"} 1\n",
            "basic_histogram{quantile=\"1\"} 1\n",
            "basic_histogram_sum 1\n",
            "basic_histogram_count 1\n\n",
        );

        assert_eq!(rendered, expected);

        mock.increment(Duration::from_secs(9));
        let rendered = handle.render();
        assert_eq!(rendered, expected);

        mock.increment(Duration::from_secs(2));
        let rendered = handle.render();
        assert_eq!(rendered, "");
    }

    #[test]
    fn test_idle_timeout_partial() {
        let (clock, mock) = Clock::mock();

        let recorder = PrometheusBuilder::new()
            .idle_timeout(
                MetricKindMask::COUNTER | MetricKindMask::HISTOGRAM,
                Some(Duration::from_secs(10)),
            )
            .set_quantiles(&[0.0, 1.0])
            .unwrap()
            .build_with_clock(clock);

        let key = Key::from_name("basic_counter");
        let counter1 = recorder.register_counter(&key, &METADATA);
        counter1.increment(42);

        let key = Key::from_name("basic_gauge");
        let gauge1 = recorder.register_gauge(&key, &METADATA);
        gauge1.set(-3.14);

        let key = Key::from_name("basic_histogram");
        let histo1 = recorder.register_histogram(&key, &METADATA);
        histo1.record(1.0);

        let handle = recorder.handle();
        let rendered = handle.render();
        let expected = concat!(
            "# TYPE basic_counter counter\n",
            "basic_counter 42\n\n",
            "# TYPE basic_gauge gauge\n",
            "basic_gauge -3.14\n\n",
            "# TYPE basic_histogram summary\n",
            "basic_histogram{quantile=\"0\"} 1\n",
            "basic_histogram{quantile=\"1\"} 1\n",
            "basic_histogram_sum 1\n",
            "basic_histogram_count 1\n\n",
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

    #[test]
    fn test_idle_timeout_staggered_distributions() {
        let (clock, mock) = Clock::mock();

        let recorder = PrometheusBuilder::new()
            .idle_timeout(MetricKindMask::ALL, Some(Duration::from_secs(10)))
            .set_quantiles(&[0.0, 1.0])
            .unwrap()
            .build_with_clock(clock);

        let key = Key::from_name("basic_counter");
        let counter1 = recorder.register_counter(&key, &METADATA);
        counter1.increment(42);

        let key = Key::from_name("basic_gauge");
        let gauge1 = recorder.register_gauge(&key, &METADATA);
        gauge1.set(-3.14);

        let key = Key::from_name("basic_histogram");
        let histo1 = recorder.register_histogram(&key, &METADATA);
        histo1.record(1.0);

        let handle = recorder.handle();
        let rendered = handle.render();
        let expected = concat!(
            "# TYPE basic_counter counter\n",
            "basic_counter 42\n\n",
            "# TYPE basic_gauge gauge\n",
            "basic_gauge -3.14\n\n",
            "# TYPE basic_histogram summary\n",
            "basic_histogram{quantile=\"0\"} 1\n",
            "basic_histogram{quantile=\"1\"} 1\n",
            "basic_histogram_sum 1\n",
            "basic_histogram_count 1\n\n",
        );

        assert_eq!(rendered, expected);

        mock.increment(Duration::from_secs(9));
        let rendered = handle.render();
        assert_eq!(rendered, expected);

        let key = Key::from_parts("basic_histogram", vec![Label::new("type", "special")]);
        let histo2 = recorder.register_histogram(&key, &METADATA);
        histo2.record(2.0);

        let expected_second = concat!(
            "# TYPE basic_counter counter\n",
            "basic_counter 42\n\n",
            "# TYPE basic_gauge gauge\n",
            "basic_gauge -3.14\n\n",
            "# TYPE basic_histogram summary\n",
            "basic_histogram{quantile=\"0\"} 1\n",
            "basic_histogram{quantile=\"1\"} 1\n",
            "basic_histogram_sum 1\n",
            "basic_histogram_count 1\n",
            "basic_histogram{type=\"special\",quantile=\"0\"} 2\n",
            "basic_histogram{type=\"special\",quantile=\"1\"} 2\n",
            "basic_histogram_sum{type=\"special\"} 2\n",
            "basic_histogram_count{type=\"special\"} 1\n\n",
        );
        let rendered = handle.render();
        assert_eq!(rendered, expected_second);

        let expected_after = concat!(
            "# TYPE basic_histogram summary\n",
            "basic_histogram{type=\"special\",quantile=\"0\"} 2\n",
            "basic_histogram{type=\"special\",quantile=\"1\"} 2\n",
            "basic_histogram_sum{type=\"special\"} 2\n",
            "basic_histogram_count{type=\"special\"} 1\n\n",
        );

        mock.increment(Duration::from_secs(2));
        let rendered = handle.render();
        assert_eq!(rendered, expected_after);
    }

    #[test]
    fn test_idle_timeout_doesnt_remove_recents() {
        let (clock, mock) = Clock::mock();

        let recorder = PrometheusBuilder::new()
            .idle_timeout(MetricKindMask::ALL, Some(Duration::from_secs(10)))
            .build_with_clock(clock);

        let key = Key::from_name("basic_counter");
        let counter1 = recorder.register_counter(&key, &METADATA);
        counter1.increment(42);

        let key = Key::from_name("basic_gauge");
        let gauge1 = recorder.register_gauge(&key, &METADATA);
        gauge1.set(-3.14);

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

        let expected_second = concat!(
            "# TYPE basic_counter counter\n",
            "basic_counter 42\n\n",
            "# TYPE basic_gauge gauge\n",
            "basic_gauge -3.14\n\n",
        );
        let rendered = handle.render();
        assert_eq!(rendered, expected_second);

        counter1.increment(1);

        let expected_after = concat!("# TYPE basic_counter counter\n", "basic_counter 43\n\n",);

        mock.increment(Duration::from_secs(2));
        let rendered = handle.render();
        assert_eq!(rendered, expected_after);
    }

    #[test]
    fn test_idle_timeout_catches_delayed_idle() {
        let (clock, mock) = Clock::mock();

        let recorder = PrometheusBuilder::new()
            .idle_timeout(MetricKindMask::ALL, Some(Duration::from_secs(10)))
            .build_with_clock(clock);

        let key = Key::from_name("basic_counter");
        let counter1 = recorder.register_counter(&key, &METADATA);
        counter1.increment(42);

        // First render, which starts tracking the counter in the recency state.
        let handle = recorder.handle();
        let rendered = handle.render();
        let expected = concat!("# TYPE basic_counter counter\n", "basic_counter 42\n\n",);

        assert_eq!(rendered, expected);

        // Now go forward by 9 seconds, which is close but still right unfer the idle timeout.
        mock.increment(Duration::from_secs(9));
        let rendered = handle.render();
        assert_eq!(rendered, expected);

        // Now increment the counter and advance time by two seconds: this pushes it over the idle
        // timeout threshold, but it should not be removed since it has been updated.
        counter1.increment(1);

        let expected_after = concat!("# TYPE basic_counter counter\n", "basic_counter 43\n\n",);

        mock.increment(Duration::from_secs(2));
        let rendered = handle.render();
        assert_eq!(rendered, expected_after);

        // Now advance by 11 seconds, right past the idle timeout threshold.  We've made no further
        // updates to the counter so it should be properly removed this time.
        mock.increment(Duration::from_secs(11));
        let rendered = handle.render();
        assert_eq!(rendered, "");
    }

    #[test]
    pub fn test_global_labels() {
        let recorder = PrometheusBuilder::new()
            .add_global_label("foo", "foo")
            .add_global_label("foo", "bar")
            .build_recorder();
        let key = Key::from_name("basic_counter");
        let counter1 = recorder.register_counter(&key, &METADATA);
        counter1.increment(42);

        let handle = recorder.handle();
        let rendered = handle.render();
        let expected_counter = "# TYPE basic_counter counter\nbasic_counter{foo=\"bar\"} 42\n\n";

        assert_eq!(rendered, expected_counter);
    }

    #[test]
    pub fn test_global_labels_overrides() {
        let recorder = PrometheusBuilder::new().add_global_label("foo", "foo").build_recorder();

        let key =
            Key::from_name("overridden").with_extra_labels(vec![Label::new("foo", "overridden")]);
        let counter1 = recorder.register_counter(&key, &METADATA);
        counter1.increment(1);

        let handle = recorder.handle();
        let rendered = handle.render();
        let expected_counter = "# TYPE overridden counter\noverridden{foo=\"overridden\"} 1\n\n";

        assert_eq!(rendered, expected_counter);
    }

    #[test]
    pub fn test_sanitized_render() {
        let recorder = PrometheusBuilder::new().add_global_label("foo:", "foo").build_recorder();

        let key_name = KeyName::from("yee_haw:lets go");
        let key = Key::from_name(key_name.clone())
            .with_extra_labels(vec![Label::new("øhno", "\"yeet\nies\\\"")]);
        recorder.describe_counter(key_name, None, "\"Simplë stuff.\nRëally.\"".into());
        let counter1 = recorder.register_counter(&key, &METADATA);
        counter1.increment(1);

        let handle = recorder.handle();
        let rendered = handle.render();
        let expected_counter = "# HELP yee_haw:lets_go \"Simplë stuff.\\nRëally.\"\n# TYPE yee_haw:lets_go counter\nyee_haw:lets_go{foo_=\"foo\",_hno=\"\\\"yeet\\nies\\\"\"} 1\n\n";

        assert_eq!(rendered, expected_counter);
    }
}
