use std::collections::HashMap;
use std::io;

use hdrhistogram::Histogram as HdrHistogram;
use hyper::Error as HyperError;
use metrics::SetRecorderError;
use metrics_util::Histogram;
use thiserror::Error as ThisError;

/// Matches a metric name in a specific way.
///
/// Used for specifying overrides for buckets, allowing a default set of histogram buckets to be
/// specified while adjusting the buckets that get used for specific metrics.
#[derive(Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
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
}

/// Errors that could occur while installing a Prometheus recorder/exporter.
#[derive(ThisError, Debug)]
pub enum InstallError {
    /// Creating the networking event loop did not succeed.
    #[error("failed to spawn Tokio runtime for endpoint: {0}")]
    Io(#[from] io::Error),

    /// Binding/listening to the given address did not succeed.
    #[cfg(feature = "tokio-exporter")]
    #[error("failed to bind to given listen address: {0}")]
    Hyper(#[from] HyperError),

    /// Installing the recorder did not succeed.
    #[error("failed to install exporter as global recorder: {0}")]
    Recorder(#[from] SetRecorderError),
}

#[derive(Clone)]
pub enum Distribution {
    /// A Prometheus histogram.
    ///
    /// Exposes "bucketed" values to Prometheus, counting the number of samples
    /// below a given threshold i.e. 100 requests faster than 20ms, 1000 requests
    /// faster than 50ms, etc.
    Histogram(Histogram),
    /// A Prometheus summary.
    ///
    /// Computes and exposes value quantiles directly to Prometheus i.e. 50% of
    /// requests were faster than 200ms, and 99% of requests were faster than
    /// 1000ms, etc.
    Summary(HdrHistogram<u64>, u64),
}

impl Distribution {
    pub fn new_histogram(buckets: &[u64]) -> Option<Distribution> {
        let hist = Histogram::new(buckets)?;
        Some(Distribution::Histogram(hist))
    }

    pub fn new_summary() -> Option<Distribution> {
        let hist = HdrHistogram::new(3).ok()?;
        Some(Distribution::Summary(hist, 0))
    }
}

pub struct Snapshot {
    pub counters: HashMap<String, HashMap<Vec<String>, u64>>,
    pub gauges: HashMap<String, HashMap<Vec<String>, f64>>,
    pub distributions: HashMap<String, HashMap<Vec<String>, Distribution>>,
}
