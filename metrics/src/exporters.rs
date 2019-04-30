//! Commonly used exporters.
//!
//! Exporters define where metric output goes: standard output, HTTP, etc.
#[cfg(feature = "metrics-exporter-log")]
pub use metrics_exporter_log::LogExporter;
