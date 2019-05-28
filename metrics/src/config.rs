use crate::Builder;
use std::time::Duration;

/// Holds the configuration for complex metric types.
pub(crate) struct MetricConfiguration {
    pub histogram_window: Duration,
    pub histogram_granularity: Duration,
}

impl MetricConfiguration {
    pub fn from_builder(builder: &Builder) -> Self {
        Self {
            histogram_window: builder.histogram_window,
            histogram_granularity: builder.histogram_granularity,
        }
    }
}
