//! Commonly used observers.
//!
//! Observers define the format of the metric output: text, JSON, etc.
#[cfg(feature = "metrics-observer-text")]
pub use metrics_observer_text::TextBuilder;

#[cfg(feature = "metrics-observer-prometheus")]
pub use metrics_observer_prometheus::PrometheusBuilder;
