use std::collections::HashMap;
#[cfg(feature = "tokio-exporter")]
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
#[cfg(feature = "tokio-exporter")]
use std::thread;
use std::time::Duration;

#[cfg(feature = "tokio-exporter")]
use hyper::{
    server::Server,
    service::{make_service_fn, service_fn},
    {Body, Error as HyperError, Response},
};
use parking_lot::RwLock;
use quanta::Clock;
#[cfg(feature = "tokio-exporter")]
use tokio::{pin, runtime, select};

use metrics_util::{parse_quantiles, MetricKindMask, Quantile, Recency, Registry};

#[cfg(feature = "tokio-exporter")]
use crate::common::InstallError;
use crate::common::Matcher;
use crate::distribution::DistributionBuilder;
use crate::recorder::{Inner, PrometheusRecorder};

/// Builder for creating and installing a Prometheus recorder/exporter.
pub struct PrometheusBuilder {
    listen_address: SocketAddr,
    quantiles: Vec<Quantile>,
    buckets: Option<Vec<f64>>,
    bucket_overrides: Option<HashMap<Matcher, Vec<f64>>>,
    idle_timeout: Option<Duration>,
    recency_mask: MetricKindMask,
    global_labels: Option<HashMap<String, String>>,
}

impl PrometheusBuilder {
    /// Creates a new [`PrometheusBuilder`].
    pub fn new() -> Self {
        let quantiles = parse_quantiles(&[0.0, 0.5, 0.9, 0.95, 0.99, 0.999, 1.0]);

        Self {
            listen_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9000),
            quantiles,
            buckets: None,
            bucket_overrides: None,
            idle_timeout: None,
            recency_mask: MetricKindMask::NONE,
            global_labels: None,
        }
    }

    /// Sets the listen address for the Prometheus scrape endpoint.
    ///
    /// The HTTP listener that is spawned will respond to GET requests on any request path.
    ///
    /// Defaults to `0.0.0.0:9000`.
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
    pub fn set_buckets(mut self, values: &[f64]) -> Self {
        self.buckets = Some(values.to_vec());
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
    pub fn set_buckets_for_metric(mut self, matcher: Matcher, values: &[f64]) -> Self {
        let buckets = self.bucket_overrides.get_or_insert_with(HashMap::new);
        buckets.insert(matcher, values.to_vec());
        self
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
    pub fn idle_timeout(mut self, mask: MetricKindMask, timeout: Option<Duration>) -> Self {
        self.idle_timeout = timeout;
        self.recency_mask = if self.idle_timeout.is_none() {
            MetricKindMask::NONE
        } else {
            mask
        };
        self
    }

    /// Adds a global label to this exporter.
    ///
    /// Global labels are applied to all metrics.  Labels defined on the metric key itself have precedence
    /// over any global labels.  If this method is called multiple times, the latest value for a given label
    /// key will be used.
    pub fn add_global_label<K, V>(mut self, key: K, value: V) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        let labels = self.global_labels.get_or_insert_with(HashMap::new);
        labels.insert(key.into(), value.into());
        self
    }

    /// Builds the recorder and exporter and installs them globally.
    ///
    /// An error will be returned if there's an issue with creating the HTTP server or with
    /// installing the recorder as the global recorder.
    #[cfg(feature = "tokio-exporter")]
    pub fn install(self) -> Result<(), InstallError> {
        let runtime = runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;

        let (recorder, exporter) = {
            let _g = runtime.enter();
            self.build_with_exporter()?
        };
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
    pub fn build(self) -> PrometheusRecorder {
        self.build_with_clock(Clock::new())
    }

    pub(crate) fn build_with_clock(self, clock: Clock) -> PrometheusRecorder {
        let inner = Inner {
            registry: Registry::new(),
            recency: Recency::new(clock, self.recency_mask, self.idle_timeout),
            distributions: RwLock::new(HashMap::new()),
            distribution_builder: DistributionBuilder::new(
                self.quantiles,
                self.buckets,
                self.bucket_overrides,
            ),
            descriptions: RwLock::new(HashMap::new()),
            global_labels: self.global_labels.unwrap_or(HashMap::new()),
        };

        PrometheusRecorder::from(inner)
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
        let address = self.listen_address;
        let recorder = self.build();
        let handle = recorder.handle();

        let server = Server::try_bind(&address)?;

        let exporter = async move {
            let make_svc = make_service_fn(move |_| {
                let handle = handle.clone();

                async move {
                    Ok::<_, HyperError>(service_fn(move |_| {
                        let handle = handle.clone();

                        async move {
                            let output = handle.render();
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

impl Default for PrometheusBuilder {
    fn default() -> Self {
        PrometheusBuilder::new()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use quanta::Clock;

    use metrics::{GaugeValue, Key, Label, Recorder};
    use metrics_util::MetricKindMask;

    use super::{Matcher, PrometheusBuilder};

    #[test]
    fn test_render() {
        let recorder = PrometheusBuilder::new().build();

        let key = Key::from_name("basic_counter");
        recorder.increment_counter(&key, 42);

        let handle = recorder.handle();
        let rendered = handle.render();
        let expected_counter = "# TYPE basic_counter counter\nbasic_counter 42\n\n";

        assert_eq!(rendered, expected_counter);

        let labels = vec![Label::new("wutang", "forever")];
        let key = Key::from_parts("basic_gauge", labels);
        recorder.update_gauge(&key, GaugeValue::Absolute(-3.14));
        let rendered = handle.render();
        let expected_gauge = format!(
            "{}# TYPE basic_gauge gauge\nbasic_gauge{{wutang=\"forever\"}} -3.14\n\n",
            expected_counter
        );

        assert_eq!(rendered, expected_gauge);

        let key = Key::from_name("basic_histogram");
        recorder.record_histogram(&key, 12.0);
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
        const DEFAULT_VALUES: [f64; 3] = [10.0, 100.0, 1000.0];
        const PREFIX_VALUES: [f64; 3] = [15.0, 105.0, 1005.0];
        const SUFFIX_VALUES: [f64; 3] = [20.0, 110.0, 1010.0];
        const FULL_VALUES: [f64; 3] = [25.0, 115.0, 1015.0];

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
            .build();

        let full_key = Key::from_name("metrics_testing_foo");
        recorder.record_histogram(&full_key, FULL_VALUES[0]);

        let prefix_key = Key::from_name("metrics_testing_bar");
        recorder.record_histogram(&prefix_key, PREFIX_VALUES[1]);

        let suffix_key = Key::from_name("metrics_testin_foo");
        recorder.record_histogram(&suffix_key, SUFFIX_VALUES[2]);

        let default_key = Key::from_name("metrics_wee");
        recorder.record_histogram(&default_key, DEFAULT_VALUES[2] + 1.0);

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
            .idle_timeout(MetricKindMask::COUNTER, Some(Duration::from_secs(10)))
            .build_with_clock(clock);

        let key = Key::from_name("basic_counter");
        recorder.increment_counter(&key, 42);

        let key = Key::from_name("basic_gauge");
        recorder.update_gauge(&key, GaugeValue::Absolute(-3.14));

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

    #[test]
    pub fn test_global_labels() {
        let recorder = PrometheusBuilder::new()
            .add_global_label("foo", "foo")
            .add_global_label("foo", "bar")
            .build();
        let key = Key::from_name("basic_counter");
        recorder.increment_counter(&key, 42);

        let handle = recorder.handle();
        let rendered = handle.render();
        let expected_counter = "# TYPE basic_counter counter\nbasic_counter{foo=\"bar\"} 42\n\n";

        assert_eq!(rendered, expected_counter);
    }

    #[test]
    pub fn test_global_labels_overrides() {
        let recorder = PrometheusBuilder::new()
            .add_global_label("foo", "foo")
            .build();

        let key =
            Key::from_name("overridden").with_extra_labels(vec![Label::new("foo", "overridden")]);
        recorder.increment_counter(&key, 1);

        let handle = recorder.handle();
        let rendered = handle.render();
        let expected_counter = "# TYPE overridden counter\noverridden{foo=\"overridden\"} 1\n\n";

        assert_eq!(rendered, expected_counter);
    }
}
