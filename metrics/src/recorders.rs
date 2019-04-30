//! Commonly used recorders.
//!
//! Recorders define the format of the metric output: text, JSON, etc.
#[cfg(feature = "metrics-recorder-text")]
pub use metrics_recorder_text::TextRecorder;

#[cfg(feature = "metrics-recorder-prometheus")]
pub use metrics_recorder_prometheus::PrometheusRecorder;
