#[cfg(feature = "metrics-recorder-text")]
pub use metrics_recorder_text::TextRecorder;

#[cfg(feature = "metrics-recorder-prometheus")]
pub use metrics_recorder_prometheus::PrometheusRecorder;
