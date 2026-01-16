use std::collections::HashMap;

use crate::{distribution::Distribution, PrometheusRecorder};

use crate::formatting::sanitize_metric_name;
use indexmap::IndexMap;
use metrics::SetRecorderError;
use thiserror::Error;

/// Matches a metric name in a specific way.
///
/// Used for specifying overrides for buckets, allowing a default set of histogram buckets to be
/// specified while adjusting the buckets that get used for specific metrics.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Matcher {
    /// Matches the entire metric name.
    Full(String),
    /// Matches the beginning of the metric name.
    Prefix(String),
    /// Matches the end of the metric name.
    Suffix(String),
}

impl Matcher {
    /// Checks if the given key matches this matcher.
    pub fn matches(&self, key: &str) -> bool {
        match self {
            Matcher::Prefix(prefix) => key.starts_with(prefix),
            Matcher::Suffix(suffix) => key.ends_with(suffix),
            Matcher::Full(full) => key == full,
        }
    }

    /// Creates a sanitized version of this matcher.
    pub(crate) fn sanitized(self) -> Matcher {
        match self {
            Matcher::Prefix(prefix) => Matcher::Prefix(sanitize_metric_name(prefix.as_str())),
            Matcher::Suffix(suffix) => Matcher::Suffix(sanitize_metric_name(suffix.as_str())),
            Matcher::Full(full) => Matcher::Full(sanitize_metric_name(full.as_str())),
        }
    }
}

/// Errors that could occur while building or installing a Prometheus recorder/exporter.
#[derive(Debug, Error)]
pub enum BuildError {
    /// There was an issue when creating the necessary Tokio runtime to launch the exporter.
    #[error("failed to create Tokio runtime for exporter: {0}")]
    FailedToCreateRuntime(String),

    /// There was an issue when creating the HTTP listener.
    #[error("failed to create HTTP listener: {0}")]
    FailedToCreateHTTPListener(String),

    /// Installing the recorder did not succeed.
    #[error("failed to install exporter as global recorder: {0}")]
    FailedToSetGlobalRecorder(#[from] SetRecorderError<PrometheusRecorder>),

    /// The given address could not be parsed successfully as an IP address/subnet.
    #[error("failed to parse address as a valid IP address/subnet: {0}")]
    InvalidAllowlistAddress(String),

    /// The given push gateway endpoint is not a valid URI.
    #[error("push gateway endpoint is not valid: {0}")]
    InvalidPushGatewayEndpoint(String),

    /// No exporter configuration was present.
    ///
    /// This generally only occurs when HTTP listener support is disabled, but no push gateway
    /// configuration was give to the builder.
    #[error("attempted to build exporter with no exporters enabled; did you disable default features and forget to enable either the `http-listener` or `push-gateway` features?")]
    MissingExporterConfiguration,

    /// Bucket bounds or quantiles were empty.
    #[error("bucket bounds/quantiles cannot be empty")]
    EmptyBucketsOrQuantiles,

    /// Bucket duration cannot be zero.
    #[error("bucket durations cannot be set to zero")]
    ZeroBucketDuration,
}

/// Represents a set of labels as structured key-value pairs
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LabelSet {
    pub(crate) labels: Vec<(String, String)>,
}

impl LabelSet {
    /// Creates a new `LabelSet` from the given key and a set of global labels.
    pub fn from_key_and_global(
        key: &metrics::Key,
        global_labels: &IndexMap<String, String>,
    ) -> Self {
        let mut labels = global_labels.clone();
        key.labels().for_each(|label| {
            labels.insert(label.key().to_string(), label.value().to_string());
        });
        Self { labels: labels.into_iter().collect() }
    }

    /// Returns `true` if the label set is empty.
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }

    /// Returns an iterator that yields the labels in a sanitized and concatenated format.
    pub fn to_strings(&self) -> impl Iterator<Item = String> + '_ {
        self.labels.iter().map(|(k, v)| {
            format!(
                "{}=\"{}\"",
                crate::formatting::sanitize_label_key(k),
                crate::formatting::sanitize_label_value(v)
            )
        })
    }
}

#[derive(Debug)]
pub struct Snapshot {
    pub counters: HashMap<String, HashMap<LabelSet, u64>>,
    pub gauges: HashMap<String, HashMap<LabelSet, f64>>,
    pub distributions: HashMap<String, IndexMap<LabelSet, Distribution>>,
}
