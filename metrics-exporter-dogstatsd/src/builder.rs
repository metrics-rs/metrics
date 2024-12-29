use std::{net::SocketAddr, sync::Arc, time::Duration};

use thiserror::Error;

use crate::{
    forwarder::{self, ForwarderConfiguration, RemoteAddr},
    recorder::DogStatsDRecorder,
    state::{State, StateConfiguration},
};

const DEFAULT_WRITE_TIMEOUT: Duration = Duration::from_secs(1);
const DEFAULT_MAX_PAYLOAD_LEN: usize = 8192;
const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_secs(3);
const DEFAULT_HISTOGRAM_RESERVOIR_SIZE: usize = 1024;

/// Errors that could occur while building or installing a DogStatsD recorder/exporter.
#[derive(Debug, Error)]
pub enum BuildError {
    /// Failed to parse the remote address.
    #[error("invalid remote address: {reason}")]
    InvalidRemoteAddress {
        /// Details about the parsing failure.
        reason: String,
    },

    /// Failed to spawn the background thread in synchronous mode.
    #[error("failed to spawn background thread for exporter in synchronous mode")]
    Backend,

    /// Failed to install the recorder due to an existing global recorder already being installed.
    #[error("failed to install exporter as global recorder")]
    FailedToInstall,
}

/// Aggregation mode.
pub enum AggregationMode {
    /// Counters and gauges are aggregated but are not sent with a timestamp.
    ///
    /// This mode still allows for reduced network traffic, but allows for scenarios where multiple instances of the
    /// metric are sent to the same Datadog Agent instance and aren't otherwise differentiated. This may be the case if
    /// Origin Detection is disabled in the Datadog Agent.
    Conservative,

    /// Counters and gauges are aggregated and sent with a timestamp.
    ///
    /// This mode allows for the most efficient processing on the Datadog Agent side, as no aggregation is performed and
    /// metrics are passed through with minimal processing. This mode should only be used when Origin Detection is
    /// enabled, or when no other instances of the application are sending metrics to the same Datadog Agent instance,
    /// as this can result in data points being overwritten if the same metric is sent multiple times with the same
    /// timestamp.
    Aggressive,
}

/// Builder for a DogStatsD exporter.
pub struct DogStatsDBuilder {
    remote_addr: RemoteAddr,
    write_timeout: Duration,
    max_payload_len: usize,
    flush_interval: Duration,
    synchronous: bool,
    agg_mode: AggregationMode,
    telemetry: bool,
    histogram_sampling: bool,
    histogram_reservoir_size: usize,
    histograms_as_distributions: bool,
}

impl DogStatsDBuilder {
    /// Set the remote address to forward metrics to.
    ///
    /// For UDP, the address simply needs to be in the format of `<host>:<port>`. For Unix domain sockets, an address in
    /// the format of `<scheme>://<path>`. The scheme can be either `unix` or `unixgram`, for a stream (`SOCK_STREAM`)
    /// or datagram (`SOCK_DGRAM`) socket, respectively.
    ///
    /// Defaults to sending to `127.0.0.1:8125` over UDP.
    ///
    /// # Errors
    ///
    /// If the given address is not able to be parsed as a valid address, an error will be returned indicating the
    /// reason.
    pub fn with_remote_address<A>(mut self, addr: A) -> Result<Self, BuildError>
    where
        A: AsRef<str>,
    {
        self.remote_addr = RemoteAddr::try_from(addr.as_ref())
            .map_err(|reason| BuildError::InvalidRemoteAddress { reason })?;
        Ok(self)
    }

    /// Set the write timeout for forwarding metrics.
    ///
    /// When the write timeout is reached, the write operation will be aborted and the payload being sent at the time
    /// will be dropped without retrying.
    ///
    /// Defaults to 1 second.
    #[must_use]
    pub fn with_write_timeout(mut self, timeout: Duration) -> Self {
        self.write_timeout = timeout;
        self
    }

    /// Set the maximum payload length for forwarding metrics.
    ///
    /// This controls the maximum size of a single payload that will be sent to the remote server. As metric payloads
    /// are being built, they will be limited to this size. If a metric cannot be built without exceeding this size, it
    /// will be dropped.
    ///
    /// This should generally be set to the same value (or lower) as `dogstatsd_buffer_size` in the Datadog Agent.
    /// Setting a higher value is likely to lead to invalid metric payloads that are discarded by the Datadog Agent when
    /// received.
    ///
    /// Defaults to 8192 bytes.
    #[must_use]
    pub fn with_maximum_payload_length(mut self, max_payload_len: usize) -> Self {
        self.max_payload_len = max_payload_len;
        self
    }

    /// Use a synchronous backend for forwarding metrics.
    ///
    /// A background OS thread will be spawned to handle forwarding metrics to the remote server.
    ///
    /// Defaults to `true`.
    #[must_use]
    pub fn with_synchronous_backend(mut self) -> Self {
        self.synchronous = true;
        self
    }

    /// Set the aggregation mode for the exporter.
    ///
    /// Counters and gauges are always aggregated locally before forwarding to the Datadog Agent, but the aggregation
    /// mode controls how much information is sent in the metric payloads. Changing the aggregation mode can potentially
    /// allow for more efficient processing on the Datadog Agent side, but is not suitable for all scenarios.
    ///
    /// See [`AggregationMode`] for more details.
    ///
    /// Defaults to [`AggregationMode::Conservative`].
    #[must_use]
    pub fn with_aggregation_mode(mut self, mode: AggregationMode) -> Self {
        self.agg_mode = mode;
        self
    }

    /// Set the flush interval of the aggregator.
    ///
    /// This controls how often metrics are forwarded to the remote server, and in turn controls the efficiency of
    /// aggregation. A shorter interval will provide more frequent updates to the remote server, but will result in more
    /// network traffic and processing overhead.
    ///
    /// Defaults to 3 seconds.
    #[must_use]
    pub fn with_flush_interval(mut self, flush_interval: Duration) -> Self {
        self.flush_interval = flush_interval;
        self
    }

    /// Sets whether or not to enable telemetry for the exporter.
    ///
    /// When enabled, additional metrics will be sent to the configured remote server that provide insight into the
    /// operation of the exporter itself, such as the number of active metrics, how many points were flushed or dropped,
    /// how many payloads and bytes were sent, and so on.
    ///
    /// Defaults to `true`.
    #[must_use]
    pub fn with_telemetry(mut self, telemetry: bool) -> Self {
        self.telemetry = telemetry;
        self
    }

    /// Sets whether or not to enable histogram sampling.
    ///
    /// When enabled, histograms utilize [reservoir sampling][reservoir] to represent any arbitrarily large number of
    /// input values using a small, fixed size array. This means that whether or not the histogram has 1,000 or
    /// 1,000,000 values recorded to itl, the memory consumption will be the same _and_ the resulting values in the
    /// histogram will be statistically representative of the overall population.
    ///
    /// When histogram sampling is enabled, each histogram metric will consume roughly `reservoir_size * 16` bytes. For
    /// example, when the reservoir size is 1,024, each histogram will consume roughly 16KB of memory. This memory is
    /// allocated for the life of a histogram and does not grow or shrink while the histogram is active, so care must be
    /// taken if there are a high number of active histograms at any given time.
    ///
    /// If your application frequently has many (100s or more) active histograms, or if your application does not have a
    /// high number of histogram updates, you likely will not benefit from enabling histogram sampling.
    ///
    /// Defaults to `false`.
    ///
    /// [reservoir]: https://en.wikipedia.org/wiki/Reservoir_sampling
    #[must_use]
    pub fn with_histogram_sampling(mut self, histogram_sampling: bool) -> Self {
        self.histogram_sampling = histogram_sampling;
        self
    }

    /// Sets the reservoir size for histogram sampling.
    ///
    /// Defaults to 1,024.
    #[must_use]
    pub fn with_histogram_reservoir_size(mut self, reservoir_size: usize) -> Self {
        self.histogram_reservoir_size = reservoir_size;
        self
    }

    /// Sets whether or not to send histograms as distributions.
    ///
    /// When enabled, histograms will be sent as distributions to the remote server. This changes the default behavior
    /// of how the metrics will be processed by the Datadog Agent, as histograms have a specific set of default "aggregates"
    /// calculated -- `max`, `median`, `avg`, `count`, etc -- locally in the Datadog Agent, whereas distributions are
    /// aggregated entirely on the Datadog backend, and provide richer support for global aggregation and specific
    /// percentiles.
    ///
    /// Generally speaking, distributions are vastly more powerful and preferred over histograms, but sending as
    /// histograms may be required to ensure parity with existing applications.
    ///
    /// Defaults to `true`.
    #[must_use]
    pub fn send_histograms_as_distributions(mut self, histograms_as_distributions: bool) -> Self {
        self.histograms_as_distributions = histograms_as_distributions;
        self
    }

    /// Builds the recorder.
    ///
    /// The configured backend will be spawned to forward metrics to the remote server, but the recorder must be
    /// manually installed by the caller.
    ///
    /// # Errors
    ///
    /// If the exporter is configured to use an asynchronous backend but is not built in the context of an asynchronous
    /// runtime, an error will be returned.
    pub fn build(self) -> Result<DogStatsDRecorder, BuildError> {
        let state_config = StateConfiguration {
            agg_mode: self.agg_mode,
            telemetry: self.telemetry,
            histogram_sampling: self.histogram_sampling,
            histogram_reservoir_size: self.histogram_reservoir_size,
            histograms_as_distributions: self.histograms_as_distributions,
        };
        let state = Arc::new(State::new(state_config));

        let recorder = DogStatsDRecorder::new(Arc::clone(&state));

        let forwarder_config = ForwarderConfiguration {
            remote_addr: self.remote_addr,
            max_payload_len: self.max_payload_len,
            flush_interval: self.flush_interval,
            write_timeout: self.write_timeout,
        };

        if self.synchronous {
            let forwarder = forwarder::sync::Forwarder::new(forwarder_config, state);

            std::thread::Builder::new()
                .name("metrics-exporter-dogstatsd-forwarder".to_string())
                .spawn(move || forwarder.run())
                .map_err(|_| BuildError::Backend)?;
        } else {
            unreachable!("Asynchronous backend should not be configurable yet.");
        }

        Ok(recorder)
    }

    /// Builds and installs the recorder.
    ///
    /// The configured backend will be spawned to forward metrics to the remote server, and the recorder will be
    /// installed as the global recorder.
    ///
    /// # Errors
    ///
    /// If the exporter is configured to use an asynchronous backend but is not built in the context of an asynchronous
    /// runtime, or if a global recorder is already installed, an error will be returned.
    pub fn install(self) -> Result<(), BuildError> {
        let recorder = self.build()?;

        metrics::set_global_recorder(recorder).map_err(|_| BuildError::FailedToInstall)
    }
}

impl Default for DogStatsDBuilder {
    fn default() -> Self {
        DogStatsDBuilder {
            remote_addr: RemoteAddr::Udp(vec![SocketAddr::from(([127, 0, 0, 1], 8125))]),
            write_timeout: DEFAULT_WRITE_TIMEOUT,
            max_payload_len: DEFAULT_MAX_PAYLOAD_LEN,
            flush_interval: DEFAULT_FLUSH_INTERVAL,
            synchronous: true,
            agg_mode: AggregationMode::Conservative,
            telemetry: true,
            histogram_sampling: false,
            histogram_reservoir_size: DEFAULT_HISTOGRAM_RESERVOIR_SIZE,
            histograms_as_distributions: true,
        }
    }
}
