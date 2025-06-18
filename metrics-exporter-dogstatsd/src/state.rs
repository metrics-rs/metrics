use std::{collections::HashSet, time::SystemTime};

use metrics::{Key, Label};
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

    /// Global labels to add to all metrics
    pub global_labels: Vec<Label>,

    /// Global prefix/namespace to use for all metrics
    pub global_prefix: Option<String>,
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
            AggregationMode::Aggressive => {
                SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).ok().map(|d| d.as_secs())
            }
            AggregationMode::Conservative => None,
        }
    }

    /// Flushes all registered metrics to the given payload writer.
    pub fn flush(
        &self,
        flush_state: &mut FlushState,
        writer: &mut PayloadWriter,
        telemetry: &mut TelemetryUpdate,
    ) {
        // TODO: Delete metrics when they are idle. (This needs support in the handles before we could do this.)

        let counters = self.registry.get_counter_handles();
        let mut active_counters = 0;

        for (key, counter) in counters {
            let (value, points_flushed) = counter.flush();

            // If the counter is already idle, and no updates were made since the last time the counter was flushed,
            // then we've already emitted our zero value and no longer need to emit updates until the counter is active
            // again.
            if points_flushed == 0 {
                if flush_state.is_counter_idle(&key) {
                    continue;
                }

                flush_state.mark_counter_as_idle(key.clone());
            } else {
                flush_state.clear_counter_idle(&key);
            }

            active_counters += 1;

            let prefix = if key.name().starts_with("datadog.dogstatsd.client") {
                None
            } else {
                self.config.global_prefix.as_deref()
            };

            let result = writer.write_counter(
                &key,
                value,
                self.get_aggregation_timestamp(),
                prefix,
                &self.config.global_labels,
            );
            if result.any_failures() {
                let points_dropped = result.points_dropped();
                error!(
                    metric_name = key.name(),
                    points_dropped, "Failed to build counter payload."
                );

                telemetry.track_packet_serializer_failed();
            } else {
                telemetry.increment_counter_points(points_flushed);
            }
        }

        telemetry.increment_counter_contexts(active_counters);

        let gauges = self.registry.get_gauge_handles();
        telemetry.increment_gauge_contexts(gauges.len());

        for (key, gauge) in gauges {
            let (value, points_flushed) = gauge.flush();
            let prefix = if key.name().starts_with("datadog.dogstatsd.client") {
                None
            } else {
                self.config.global_prefix.as_deref()
            };
            let result = writer.write_gauge(
                &key,
                value,
                self.get_aggregation_timestamp(),
                prefix,
                &self.config.global_labels,
            );
            if result.any_failures() {
                let points_dropped = result.points_dropped();
                error!(metric_name = key.name(), points_dropped, "Failed to build gauge payload.");

                telemetry.track_packet_serializer_failed();
            } else {
                telemetry.increment_gauge_points(points_flushed);
            }
        }

        let histograms = self.registry.get_histogram_handles();
        let mut active_histograms = 0;

        for (key, histogram) in histograms {
            if histogram.is_empty() {
                continue;
            }

            active_histograms += 1;
            let prefix = if key.name().starts_with("datadog.dogstatsd.client") {
                None
            } else {
                self.config.global_prefix.as_deref()
            };

            histogram.flush(|maybe_sample_rate, values| {
                let points_len = values.len();
                let result = if self.config.histograms_as_distributions {
                    writer.write_distribution(
                        &key,
                        values,
                        maybe_sample_rate,
                        prefix,
                        &self.config.global_labels,
                    )
                } else {
                    writer.write_histogram(
                        &key,
                        values,
                        maybe_sample_rate,
                        prefix,
                        &self.config.global_labels,
                    )
                };

                // Scale the points flushed/dropped values by the sample rate to determine the true number of points flushed/dropped.
                let sample_rate = maybe_sample_rate.unwrap_or(1.0);
                let points_flushed =
                    ((points_len as u64 - result.points_dropped()) as f64 / sample_rate) as u64;
                telemetry.increment_histogram_points(points_flushed);

                let points_dropped = (result.points_dropped() as f64 / sample_rate) as u64;

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

                    telemetry.track_packet_serializer_failed();
                }
            });
        }

        telemetry.increment_histogram_contexts(active_histograms);
    }
}

/// Flush state.
///
/// This type contains state information related to flush operations, and is intended to be held by the forwarder and
/// used during each flush. This allows the flush state information to be updated without needing to wrap calls to
/// `State` within a lock.
#[derive(Default)]
pub struct FlushState {
    idle_counters: HashSet<Key>,
}

impl FlushState {
    /// Marks a counter as idle.
    fn mark_counter_as_idle(&mut self, key: Key) {
        self.idle_counters.insert(key);
    }

    fn clear_counter_idle(&mut self, key: &Key) {
        self.idle_counters.remove(key);
    }

    /// Returns `true` if the counter is idle.
    fn is_counter_idle(&self, key: &Key) -> bool {
        self.idle_counters.contains(key)
    }
}
