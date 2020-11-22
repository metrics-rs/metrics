#[cfg(feature = "tokio-exporter")]
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
#[cfg(feature = "tokio-exporter")]
use std::thread;
use std::time::Duration;
use std::{collections::HashMap, sync::Arc};

use crate::common::{InstallError, Matcher, MetricType};
use crate::recorder::{Inner, PrometheusRecorder};

#[cfg(feature = "tokio-exporter")]
use hyper::{
    service::{make_service_fn, service_fn},
    {Body, Error as HyperError, Response, Server},
};
use metrics_util::{parse_quantiles, Quantile, Registry};
use parking_lot::{Mutex, RwLock};
use quanta::Clock;
#[cfg(feature = "tokio-exporter")]
use tokio::{pin, runtime, select};

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
    pub fn install(self) -> Result<(), InstallError> {
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
    pub fn build(self) -> Result<PrometheusRecorder, InstallError> {
        self.build_with_clock(Clock::new())
    }

    pub(crate) fn build_with_clock(self, clock: Clock) -> Result<PrometheusRecorder, InstallError> {
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

        Ok(PrometheusRecorder::from(inner))
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
        InstallError,
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

        let recorder = PrometheusRecorder::from(inner.clone());

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
