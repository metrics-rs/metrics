use std::collections::HashMap;
#[cfg(feature = "push-gateway")]
use std::convert::TryFrom;
#[cfg(any(feature = "http-listener", feature = "push-gateway"))]
use std::future::Future;
#[cfg(feature = "http-listener")]
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
#[cfg(any(feature = "http-listener", feature = "push-gateway"))]
use std::pin::Pin;
use std::sync::RwLock;
#[cfg(any(feature = "http-listener", feature = "push-gateway"))]
use std::thread;
use std::time::Duration;

#[cfg(any(feature = "http-listener", feature = "push-gateway"))]
use hyper::Body;

#[cfg(feature = "http-listener")]
use hyper::{
    server::{conn::AddrStream, Server},
    service::{make_service_fn, service_fn},
    Response, StatusCode,
};

#[cfg(feature = "push-gateway")]
use hyper::{
    body::{aggregate, Buf},
    client::Client,
    http::HeaderValue,
    Method, Request, Uri,
};

use indexmap::IndexMap;
#[cfg(feature = "http-listener")]
use ipnet::IpNet;
use quanta::Clock;
#[cfg(any(feature = "http-listener", feature = "push-gateway"))]
use tokio::runtime;
#[cfg(feature = "push-gateway")]
use tracing::error;

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

#[cfg(any(feature = "http-listener", feature = "push-gateway"))]
type ExporterFuture = Pin<Box<dyn Future<Output = Result<(), hyper::Error>> + Send + 'static>>;

#[derive(Clone)]
enum ExporterConfig {
    // Run an HTTP listener on the given `listen_address`.
    #[cfg(feature = "http-listener")]
    HttpListener { listen_address: SocketAddr },

    // Run a push gateway task sending to the given `endpoint` after `interval` time has elapsed,
    // infinitely.
    #[cfg(feature = "push-gateway")]
    PushGateway {
        endpoint: Uri,
        interval: Duration,
        username: Option<String>,
        password: Option<String>,
    },

    #[allow(dead_code)]
    Unconfigured,
}

impl ExporterConfig {
    #[cfg_attr(not(any(feature = "http-listener", feature = "push-gateway")), allow(dead_code))]
    fn as_type_str(&self) -> &'static str {
        match self {
            #[cfg(feature = "http-listener")]
            Self::HttpListener { .. } => "http-listener",
            #[cfg(feature = "push-gateway")]
            Self::PushGateway { .. } => "push-gateway",
            Self::Unconfigured => "unconfigured,",
        }
    }
}

/// Builder for creating and installing a Prometheus recorder/exporter.
pub struct PrometheusBuilder {
    #[cfg_attr(not(any(feature = "http-listener", feature = "push-gateway")), allow(dead_code))]
    exporter_config: ExporterConfig,
    #[cfg(feature = "http-listener")]
    allowed_addresses: Option<Vec<IpNet>>,
    quantiles: Vec<Quantile>,
    buckets: Option<Vec<f64>>,
    bucket_overrides: Option<HashMap<Matcher, Vec<f64>>>,
    idle_timeout: Option<Duration>,
    recency_mask: MetricKindMask,
    global_labels: Option<IndexMap<String, String>>,
}

impl PrometheusBuilder {
    /// Creates a new [`PrometheusBuilder`].
    pub fn new() -> Self {
        let quantiles = parse_quantiles(&[0.0, 0.5, 0.9, 0.95, 0.99, 0.999, 1.0]);

        #[cfg(feature = "http-listener")]
        let exporter_config = ExporterConfig::HttpListener {
            listen_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9000),
        };
        #[cfg(not(feature = "http-listener"))]
        let exporter_config = ExporterConfig::Unconfigured;

        Self {
            exporter_config,
            #[cfg(feature = "http-listener")]
            allowed_addresses: None,
            quantiles,
            buckets: None,
            bucket_overrides: None,
            idle_timeout: None,
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
        self.exporter_config = ExporterConfig::HttpListener { listen_address: addr.into() };
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

        metrics::set_boxed_recorder(Box::new(recorder))?;

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

        metrics::set_boxed_recorder(Box::new(recorder))?;

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
    #[cfg(any(feature = "http-listener", feature = "push-gateway"))]
    #[cfg_attr(docsrs, doc(cfg(any(feature = "http-listener", feature = "push-gateway"))))]
    #[cfg_attr(not(feature = "http-listener"), allow(unused_mut))]
    pub fn build(mut self) -> Result<(PrometheusRecorder, ExporterFuture), BuildError> {
        #[cfg(feature = "http-listener")]
        let allowed_addresses = self.allowed_addresses.take();

        let exporter_config = self.exporter_config.clone();
        let recorder = self.build_recorder();
        let handle = recorder.handle();

        match exporter_config {
            ExporterConfig::Unconfigured => Err(BuildError::MissingExporterConfiguration),
            #[cfg(feature = "http-listener")]
            ExporterConfig::HttpListener { listen_address } => {
                let server = Server::try_bind(&listen_address)
                    .map_err(|e| BuildError::FailedToCreateHTTPListener(e.to_string()))?;
                let exporter = async move {
                    let make_svc = make_service_fn(move |socket: &AddrStream| {
                        let remote_addr = socket.remote_addr().ip();

                        // If the allowlist is empty, the request is allowed.  Otherwise, it must
                        // match one of the entries in the allowlist or it will be denied.
                        let is_allowed = allowed_addresses.as_ref().map_or(true, |addresses| {
                            addresses.iter().any(|address| address.contains(&remote_addr))
                        });

                        let handle = handle.clone();

                        async move {
                            Ok::<_, hyper::Error>(service_fn(move |_| {
                                let handle = handle.clone();

                                async move {
                                    if is_allowed {
                                        let output = handle.render();
                                        Ok::<_, hyper::Error>(Response::new(Body::from(output)))
                                    } else {
                                        Ok::<_, hyper::Error>(
                                            Response::builder()
                                                .status(StatusCode::FORBIDDEN)
                                                .body(Body::empty())
                                                .expect("static response is valid"),
                                        )
                                    }
                                }
                            }))
                        }
                    });
                    server.serve(make_svc).await
                };

                Ok((recorder, Box::pin(exporter)))
            }

            #[cfg(feature = "push-gateway")]
            ExporterConfig::PushGateway { endpoint, interval, username, password } => {
                let exporter = async move {
                    let client = Client::new();
                    let auth = username
                        .as_ref()
                        .map(|name| basic_auth(name, password.as_ref().map(|x| &**x)));

                    loop {
                        // Sleep for `interval` amount of time, and then do a push.
                        tokio::time::sleep(interval).await;

                        let mut builder = Request::builder();
                        if let Some(auth) = &auth {
                            builder = builder.header("authorization", auth.clone());
                        }

                        let output = handle.render();
                        let result = builder
                            .method(Method::PUT)
                            .uri(endpoint.clone())
                            .body(Body::from(output));
                        let req = match result {
                            Ok(req) => req,
                            Err(e) => {
                                error!("failed to build push gateway request: {}", e);
                                continue;
                            }
                        };

                        match client.request(req).await {
                            Ok(response) => {
                                if !response.status().is_success() {
                                    let status = response.status();
                                    let status = status
                                        .canonical_reason()
                                        .unwrap_or_else(|| status.as_str());
                                    let body = aggregate(response.into_body()).await;
                                    let body = body
                                        .map_err(|_| ())
                                        .map(|mut b| b.copy_to_bytes(b.remaining()))
                                        .map(|b| b[..].to_vec())
                                        .and_then(|s| String::from_utf8(s).map_err(|_| ()))
                                        .unwrap_or_else(|_| {
                                            String::from("<failed to read response body>")
                                        });
                                    error!(
                                        message = "unexpected status after pushing metrics to push gateway",
                                        status,
                                        %body,
                                    );
                                }
                            }
                            Err(e) => error!("error sending request to push gateway: {:?}", e),
                        }
                    }
                };

                Ok((recorder, Box::pin(exporter)))
            }
        }
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
                self.buckets,
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

#[cfg(feature = "push-gateway")]
fn basic_auth(username: &str, password: Option<&str>) -> HeaderValue {
    use base64::prelude::BASE64_STANDARD;
    use base64::write::EncoderWriter;
    use std::io::Write;

    let mut buf = b"Basic ".to_vec();
    {
        let mut encoder = EncoderWriter::new(&mut buf, &BASE64_STANDARD);
        let _ = write!(encoder, "{username}:");
        if let Some(password) = password {
            let _ = write!(encoder, "{password}");
        }
    }
    let mut header = HeaderValue::from_bytes(&buf).expect("base64 is always valid HeaderValue");
    header.set_sensitive(true);
    header
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use quanta::Clock;

    use metrics::{Key, KeyName, Label, Recorder};
    use metrics_util::MetricKindMask;

    use super::{Matcher, PrometheusBuilder};

    #[test]
    fn test_render() {
        let recorder =
            PrometheusBuilder::new().set_quantiles(&[0.0, 1.0]).unwrap().build_recorder();

        let key = Key::from_name("basic_counter");
        let counter1 = recorder.register_counter(&key);
        counter1.increment(42);

        let handle = recorder.handle();
        let rendered = handle.render();
        let expected_counter = "# TYPE basic_counter counter\nbasic_counter 42\n\n";

        assert_eq!(rendered, expected_counter);

        let labels = vec![Label::new("wutang", "forever")];
        let key = Key::from_parts("basic_gauge", labels);
        let gauge1 = recorder.register_gauge(&key);
        gauge1.set(-3.14);
        let rendered = handle.render();
        let expected_gauge = format!(
            "{}# TYPE basic_gauge gauge\nbasic_gauge{{wutang=\"forever\"}} -3.14\n\n",
            expected_counter
        );

        assert_eq!(rendered, expected_gauge);

        let key = Key::from_name("basic_histogram");
        let histogram1 = recorder.register_histogram(&key);
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
        let expected_histogram = format!("{}{}", expected_gauge, histogram_data);

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
        let full_key_histo = recorder.register_histogram(&full_key);
        full_key_histo.record(FULL_VALUES[0]);

        let prefix_key = Key::from_name("metrics.testing_bar");
        let prefix_key_histo = recorder.register_histogram(&prefix_key);
        prefix_key_histo.record(PREFIX_VALUES[1]);

        let suffix_key = Key::from_name("metrics_testin_foo");
        let suffix_key_histo = recorder.register_histogram(&suffix_key);
        suffix_key_histo.record(SUFFIX_VALUES[2]);

        let default_key = Key::from_name("metrics.wee");
        let default_key_histo = recorder.register_histogram(&default_key);
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
        let counter1 = recorder.register_counter(&key);
        counter1.increment(42);

        let key = Key::from_name("basic_gauge");
        let gauge1 = recorder.register_gauge(&key);
        gauge1.set(-3.14);

        let key = Key::from_name("basic_histogram");
        let histo1 = recorder.register_histogram(&key);
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
        let counter1 = recorder.register_counter(&key);
        counter1.increment(42);

        let key = Key::from_name("basic_gauge");
        let gauge1 = recorder.register_gauge(&key);
        gauge1.set(-3.14);

        let key = Key::from_name("basic_histogram");
        let histo1 = recorder.register_histogram(&key);
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
        let counter1 = recorder.register_counter(&key);
        counter1.increment(42);

        let key = Key::from_name("basic_gauge");
        let gauge1 = recorder.register_gauge(&key);
        gauge1.set(-3.14);

        let key = Key::from_name("basic_histogram");
        let histo1 = recorder.register_histogram(&key);
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
        let histo2 = recorder.register_histogram(&key);
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
        let counter1 = recorder.register_counter(&key);
        counter1.increment(42);

        let key = Key::from_name("basic_gauge");
        let gauge1 = recorder.register_gauge(&key);
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
        let counter1 = recorder.register_counter(&key);
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
        let counter1 = recorder.register_counter(&key);
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
        let counter1 = recorder.register_counter(&key);
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
        let counter1 = recorder.register_counter(&key);
        counter1.increment(1);

        let handle = recorder.handle();
        let rendered = handle.render();
        let expected_counter = "# HELP yee_haw:lets_go \"Simplë stuff.\\nRëally.\"\n# TYPE yee_haw:lets_go counter\nyee_haw:lets_go{foo_=\"foo\",_hno=\"\\\"yeet\\nies\\\"\"} 1\n\n";

        assert_eq!(rendered, expected_counter);
    }
}

#[cfg(all(test, feature = "push-gateway"))]
mod push_gateway_tests {
    use crate::builder::basic_auth;

    #[test]
    pub fn test_basic_auth() {
        use base64::prelude::BASE64_STANDARD;
        use base64::read::DecoderReader;
        use std::io::Read;

        const BASIC: &str = "Basic ";

        // username only
        let username = "metrics";
        let header = basic_auth(username, None);

        let reader = &header.as_ref()[BASIC.len()..];
        let mut decoder = DecoderReader::new(reader, &BASE64_STANDARD);
        let mut result = Vec::new();
        decoder.read_to_end(&mut result).unwrap();
        assert_eq!(b"metrics:", &result[..]);
        assert!(header.is_sensitive());

        // username/password
        let password = "123!_@ABC";
        let header = basic_auth(username, Some(password));

        let reader = &header.as_ref()[BASIC.len()..];
        let mut decoder = DecoderReader::new(reader, &BASE64_STANDARD);
        let mut result = Vec::new();
        decoder.read_to_end(&mut result).unwrap();
        assert_eq!(b"metrics:123!_@ABC", &result[..]);
        assert!(header.is_sensitive());
    }
}
