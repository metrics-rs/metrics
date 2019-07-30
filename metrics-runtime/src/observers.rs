//! Commonly used observers.
//!
//! Observers define the format of the metric output: YAML, JSON, etc.
#[cfg(feature = "metrics-observer-yaml")]
pub use metrics_observer_yaml::YamlBuilder;

#[cfg(feature = "metrics-observer-json")]
pub use metrics_observer_json::JsonBuilder;

#[cfg(feature = "metrics-observer-prometheus")]
pub use metrics_observer_prometheus::PrometheusBuilder;
