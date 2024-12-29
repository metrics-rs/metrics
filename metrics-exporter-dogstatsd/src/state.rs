use std::time::SystemTime;

use metrics::Key;
use metrics_util::registry::Registry;
use tracing::error;

use crate::{
    builder::AggregationMode, storage::ClientSideAggregatedStorage, telemetry::TelemetryUpdate,
    writer::PayloadWriter,
};

/// Exporter state configuration.
pub struct StateConfiguration {
    /// Aggregation mode when flushing counters and gauges.
    ///
    /// See [`AggregationMode`] for more information.
    pub agg_mode: AggregationMode,

    /// Whether or not to collect/emit internal telemetry.
    pub telemetry: bool,

    /// Whether or not to sample histograms.
    pub histogram_sampling: bool,

    /// Reservoir size when histogram sampling is enabled.
    pub histogram_reservoir_size: usize,

    /// Whether or not to emit histograms as distributions.
    pub histograms_as_distributions: bool,
}

/// Exporter state.
pub(crate) struct State {
    config: StateConfiguration,
    registry: Registry<Key, ClientSideAggregatedStorage>,
}

impl State {
    /// Creates a new `State` from the given configuration.
    pub fn new(config: StateConfiguration) -> Self {
        State {
            registry: Registry::new(ClientSideAggregatedStorage::new(
                config.histogram_sampling,
                config.histogram_reservoir_size,
            )),
            config,
        }
    }

    /// Returns a reference to the registry.
    pub fn registry(&self) -> &Registry<Key, ClientSideAggregatedStorage> {
        &self.registry
    }

    /// Returns `true` if telemetry is enabled.
    pub fn telemetry_enabled(&self) -> bool {
        self.config.telemetry
    }

    fn get_aggregation_timestamp(&self) -> Option<u64> {
        match self.config.agg_mode {
            AggregationMode::Conservative => {
                SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).ok().map(|d| d.as_secs())
            }
            AggregationMode::Aggressive => None,
        }
    }

    /// Flushes all registered metrics to the given payload writer.
    pub fn flush(&self, writer: &mut PayloadWriter, telemetry: &mut TelemetryUpdate) {
        // TODO: Stop reporting metrics when they are idle.
        // TODO: Delete metrics when they are idle. (This needs support in the handles before we could do this.)

        let counters = self.registry.get_counter_handles();
        telemetry.increment_counter_contexts(counters.len());

        for (key, counter) in counters {
            let (value, points_flushed) = counter.flush();
            let result = writer.write_counter(&key, value, self.get_aggregation_timestamp());
            if result.any_failures() {
                let points_dropped = result.points_dropped();
                error!(
                    metric_name = key.name(),
                    points_dropped, "Failed to build counter payload."
                );

                // TODO: Existing DogStatsD clients don't emit a telemetry metric for metrics that were too big to even
                // fit within the maximum buffer size... so should we just come up with a new metric with the same
                // naming style, or ignore entirely for now?
            } else {
                telemetry.increment_counter_points(points_flushed);
            }
        }

        let gauges = self.registry.get_gauge_handles();
        telemetry.increment_gauge_contexts(gauges.len());

        for (key, gauge) in gauges {
            let (value, points_flushed) = gauge.flush();
            let result = writer.write_gauge(&key, value, self.get_aggregation_timestamp());
            if result.any_failures() {
                let points_dropped = result.points_dropped();
                error!(metric_name = key.name(), points_dropped, "Failed to build gauge payload.");

                // TODO: Existing DogStatsD clients don't emit a telemetry metric for metrics that were too big to even
                // fit within the maximum buffer size... so should we just come up with a new metric with the same
                // naming style, or ignore entirely for now?
            } else {
                telemetry.increment_gauge_points(points_flushed);
            }
        }

        let histograms = self.registry.get_histogram_handles();
        telemetry.increment_histogram_contexts(histograms.len());

        for (key, histogram) in histograms {
            histogram.flush(|maybe_sample_rate, values| {
                let points_len = values.len();
                let result = if self.config.histograms_as_distributions {
                    writer.write_distribution(&key, values, maybe_sample_rate)
                } else {
                    writer.write_histogram(&key, values, maybe_sample_rate)
                };

                // Scale the points flushed/dropped values by the sample rate to determine the true number of points flushed/dropped.
                let sample_rate = maybe_sample_rate.unwrap_or(1.0);
                let points_flushed =
                    ((points_len as u64 - result.points_dropped()) as f64 / sample_rate) as u64;
                telemetry.increment_histogram_points(points_flushed);

                let points_dropped = (result.points_dropped() as f64 / sample_rate) as u64;

                // TODO: Existing DogStatsD clients don't emit a telemetry metric for metrics that were too big to even
                // fit within the maximum buffer size... so should we just come up with a new metric with the same
                // naming style, or ignore entirely for now?

                if result.any_failures() {
                    if result.payloads_written() > 0 {
                        error!(
                            metric_name = key.name(),
                            points_dropped, "Failed to build some histogram payload(s)."
                        );
                    } else {
                        error!(
                            metric_name = key.name(),
                            points_dropped, "Failed to build any histogram payload(s)."
                        );
                    }
                }
            });
        }
    }
}
