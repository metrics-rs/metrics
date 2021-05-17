use std::collections::HashMap;
use std::io;

use crate::distribution::Distribution;

use metrics::SetRecorderError;
use thiserror::Error as ThisError;

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
            Matcher::Prefix(prefix) => Matcher::Prefix(sanitize_key_name(prefix.as_str())),
            Matcher::Suffix(suffix) => Matcher::Suffix(sanitize_key_name(suffix.as_str())),
            Matcher::Full(full) => Matcher::Full(sanitize_key_name(full.as_str())),
        }
    }
}

/// Errors that could occur while installing a Prometheus recorder/exporter.
#[derive(Debug, ThisError)]
pub enum InstallError {
    /// Creating the networking event loop did not succeed.
    #[error("failed to spawn Tokio runtime for endpoint: {0}")]
    Io(#[from] io::Error),

    /// Binding/listening to the given address did not succeed.
    #[cfg(feature = "tokio-exporter")]
    #[error("failed to bind to given listen address: {0}")]
    Hyper(#[from] hyper::Error),

    /// Installing the recorder did not succeed.
    #[error("failed to install exporter as global recorder: {0}")]
    Recorder(#[from] SetRecorderError),
}

pub struct Snapshot {
    pub counters: HashMap<String, HashMap<Vec<String>, u64>>,
    pub gauges: HashMap<String, HashMap<Vec<String>, f64>>,
    pub distributions: HashMap<String, HashMap<Vec<String>, Distribution>>,
}

pub fn sanitize_key_name(key: &str) -> String {
    // Replace anything that isn't [a-zA-Z0-9_:].
    let sanitize = |c: char| !(c.is_alphanumeric() || c == '_' || c == ':');
    key.to_string().replace(sanitize, "_")
}

#[cfg(test)]
mod tests {
    use super::sanitize_key_name;

    #[test]
    fn test_sanitize_key_name() {
        let test_cases = vec![
            ("____", "____"),
            ("foo bar", "foo_bar"),
            ("abcd:efgh", "abcd:efgh"),
            ("lars.andersen", "lars_andersen"),
        ];

        for (input, expected) in test_cases {
            let result = sanitize_key_name(input);
            assert_eq!(expected, result);
        }
    }
}
