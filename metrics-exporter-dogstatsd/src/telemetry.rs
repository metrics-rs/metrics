use metrics::{counter, Counter};

/// Exporter telemetry.
///
/// `Telemetry` collects information about the exporter's behavior and can be optionally enabled to send this
/// information as normal metrics to the same DogStatsD server.
pub struct Telemetry {
    metric_points: Counter,
    counter_points: Counter,
    gauge_points: Counter,
    histogram_points: Counter,
    packets_sent: Counter,
    packets_dropped: Counter,
    packets_dropped_writer: Counter,
    packets_dropped_serializer: Counter,
    bytes_dropped: Counter,
    bytes_sent: Counter,
    bytes_dropped_writer: Counter,
    agg_contexts: Counter,
    agg_contexts_counter: Counter,
    agg_contexts_gauge: Counter,
    agg_contexts_histogram: Counter,
}

impl Telemetry {
    /// Creates a `Telemetry` instance.
    pub fn new(transport: &'static str) -> Self {
        let base_labels = telemetry_tags!("client_transport" => transport);
        let counter_labels =
            telemetry_tags!("client_transport" => transport, "metrics_type" => "count");
        let gauge_labels =
            telemetry_tags!("client_transport" => transport, "metrics_type" => "gauge");
        let histogram_labels =
            telemetry_tags!("client_transport" => transport, "metrics_type" => "histogram");

        Self {
            metric_points: counter!("datadog.dogstatsd.client.metrics", base_labels.iter()),
            counter_points: counter!(
                "datadog.dogstatsd.client.metrics_by_type",
                counter_labels.iter()
            ),
            gauge_points: counter!("datadog.dogstatsd.client.metrics_by_type", gauge_labels.iter()),
            histogram_points: counter!(
                "datadog.dogstatsd.client.metrics_by_type",
                histogram_labels.iter()
            ),
            packets_sent: counter!("datadog.dogstatsd.client.packets_sent", base_labels.iter()),
            packets_dropped: counter!(
                "datadog.dogstatsd.client.packets_dropped",
                base_labels.iter()
            ),
            packets_dropped_writer: counter!(
                "datadog.dogstatsd.client.packets_dropped_writer",
                base_labels.iter()
            ),
            packets_dropped_serializer: counter!(
                "datadog.dogstatsd.client.packets_dropped_serializer",
                base_labels.iter()
            ),
            bytes_dropped: counter!("datadog.dogstatsd.client.bytes_dropped", base_labels.iter()),
            bytes_sent: counter!("datadog.dogstatsd.client.bytes_sent", base_labels.iter()),
            bytes_dropped_writer: counter!(
                "datadog.dogstatsd.client.bytes_dropped_writer",
                base_labels.iter()
            ),
            agg_contexts: counter!(
                "datadog.dogstatsd.client.aggregated_context",
                base_labels.iter()
            ),
            agg_contexts_counter: counter!(
                "datadog.dogstatsd.client.aggregated_context_by_type",
                counter_labels.iter()
            ),
            agg_contexts_gauge: counter!(
                "datadog.dogstatsd.client.aggregated_context_by_type",
                gauge_labels.iter()
            ),
            agg_contexts_histogram: counter!(
                "datadog.dogstatsd.client.aggregated_context_by_type",
                histogram_labels.iter()
            ),
        }
    }

    /// Applies the given telemetry update, updating the internal metrics.
    pub fn apply_update(&mut self, update: &TelemetryUpdate) {
        let metric_points = update.counter_points + update.gauge_points + update.histogram_points;
        let agg_contexts =
            update.counter_contexts + update.gauge_contexts + update.histogram_contexts;

        self.metric_points.increment(metric_points);
        self.counter_points.increment(update.counter_points);
        self.gauge_points.increment(update.gauge_points);
        self.histogram_points.increment(update.histogram_points);
        self.packets_sent.increment(update.packets_sent);
        self.packets_dropped.increment(update.packets_dropped);
        self.packets_dropped_writer.increment(update.packets_dropped_writer);
        self.packets_dropped_serializer.increment(update.packets_dropped_serializer);
        self.bytes_dropped.increment(update.bytes_dropped);
        self.bytes_sent.increment(update.bytes_sent);
        self.bytes_dropped_writer.increment(update.bytes_dropped_writer);
        self.agg_contexts.increment(agg_contexts);
        self.agg_contexts_counter.increment(update.counter_contexts);
        self.agg_contexts_gauge.increment(update.gauge_contexts);
        self.agg_contexts_histogram.increment(update.histogram_contexts);
    }
}

/// A buffer for collecting telemetry updates.
#[derive(Default)]
pub struct TelemetryUpdate {
    counter_contexts: u64,
    gauge_contexts: u64,
    histogram_contexts: u64,
    counter_points: u64,
    gauge_points: u64,
    histogram_points: u64,
    packets_sent: u64,
    packets_dropped: u64,
    packets_dropped_writer: u64,
    packets_dropped_serializer: u64,
    bytes_sent: u64,
    bytes_dropped: u64,
    bytes_dropped_writer: u64,
}

impl TelemetryUpdate {
    /// Clears the update buffer, resetting it back to an empty state.
    pub fn clear(&mut self) {
        self.counter_contexts = 0;
        self.gauge_contexts = 0;
        self.histogram_contexts = 0;
        self.counter_points = 0;
        self.gauge_points = 0;
        self.histogram_points = 0;
        self.packets_sent = 0;
        self.packets_dropped = 0;
        self.packets_dropped_writer = 0;
        self.packets_dropped_serializer = 0;
        self.bytes_sent = 0;
        self.bytes_dropped = 0;
        self.bytes_dropped_writer = 0;
    }

    /// Returns `true` if any updates have been recorded.
    pub fn had_updates(&self) -> bool {
        self.counter_points > 0 || self.gauge_points > 0 || self.histogram_points > 0
    }

    /// Increments the number of counter contexts collected.
    pub fn increment_counter_contexts(&mut self, value: usize) {
        self.counter_contexts += value as u64;
    }

    /// Increments the number of gauge contexts collected.
    pub fn increment_gauge_contexts(&mut self, value: usize) {
        self.gauge_contexts += value as u64;
    }

    /// Increments the number of histogram contexts collected.
    pub fn increment_histogram_contexts(&mut self, value: usize) {
        self.histogram_contexts += value as u64;
    }

    /// Increments the number of counter points collected.
    pub fn increment_counter_points(&mut self, value: u64) {
        self.counter_points += value;
    }

    /// Increments the number of gauge points collected.
    pub fn increment_gauge_points(&mut self, value: u64) {
        self.gauge_points += value;
    }

    /// Increments the number of histogram points collected.
    pub fn increment_histogram_points(&mut self, value: u64) {
        self.histogram_points += value;
    }

    /// Tracks a successful packet send.
    pub fn track_packet_send_succeeded(&mut self, bytes_len: usize) {
        self.packets_sent += 1;
        self.bytes_sent += bytes_len as u64;
    }

    /// Tracks a failed packet send.
    pub fn track_packet_send_failed(&mut self, bytes_len: usize) {
        self.packets_dropped += 1;
        self.packets_dropped_writer += 1;
        self.bytes_dropped += bytes_len as u64;
        self.bytes_dropped_writer += bytes_len as u64;
    }

    /// Tracks a failed packet serialization.
    pub fn track_packet_serializer_failed(&mut self) {
        self.packets_dropped += 1;
        self.packets_dropped_serializer += 1;
    }
}

macro_rules! _telemetry_tags {
    ($($k:literal => $v:expr),*) => {
        [
            ::metrics::Label::from_static_parts("client", "rust"),
            ::metrics::Label::from_static_parts("client_version", env!("CARGO_PKG_VERSION")),
            $(::metrics::Label::from_static_parts($k, $v),)*
        ]
    };
}

pub(crate) use _telemetry_tags as telemetry_tags;
