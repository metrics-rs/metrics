use std::collections::HashMap;

use crate::distribution::Distribution;

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
    FailedToSetGlobalRecorder(#[from] SetRecorderError),

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
}

pub struct Snapshot {
    pub counters: HashMap<String, HashMap<Vec<String>, u64>>,
    pub gauges: HashMap<String, HashMap<Vec<String>, f64>>,
    pub distributions: HashMap<String, IndexMap<Vec<String>, Distribution>>,
}

#[inline]
fn invalid_metric_name_start_character(c: char) -> bool {
    // Essentially, needs to match the regex pattern of [a-zA-Z_:].
    !(c.is_ascii_alphabetic() || c == '_' || c == ':')
}

#[inline]
fn invalid_metric_name_character(c: char) -> bool {
    // Essentially, needs to match the regex pattern of [a-zA-Z0-9_:].
    !(c.is_ascii_alphanumeric() || c == '_' || c == ':')
}

#[inline]
fn invalid_label_key_start_character(c: char) -> bool {
    // Essentially, needs to match the regex pattern of [a-zA-Z_].
    !(c.is_ascii_alphabetic() || c == '_')
}

#[inline]
fn invalid_label_key_character(c: char) -> bool {
    // Essentially, needs to match the regex pattern of [a-zA-Z0-9_].
    !(c.is_ascii_alphanumeric() || c == '_')
}

/// Sanitizes a metric name to be prometheus compatible.
pub fn sanitize_metric_name(name: &str) -> String {
    // The first character must be [a-zA-Z_:], and all subsequent characters must be [a-zA-Z0-9_:].
    name.replacen(invalid_metric_name_start_character, "_", 1)
        .replace(invalid_metric_name_character, "_")
}

/// Sanitizes an label key to be prometheus compatible.
pub fn sanitize_label_key(key: &str) -> String {
    // The first character must be [a-zA-Z_], and all subsequent characters must be [a-zA-Z0-9_].
    key.replacen(invalid_label_key_start_character, "_", 1)
        .replace(invalid_label_key_character, "_")
        .replacen("__", "___", 1)
}

/// Sanitizes an label value to be prometheus compatible.
pub fn sanitize_label_value(value: &str) -> String {
    sanitize_label_value_or_descpiption(value, false)
}

/// Sanitizes a description string to be prometheus compatible.
pub fn sanitize_description(value: &str) -> String {
    sanitize_label_value_or_descpiption(value, true)
}

fn sanitize_label_value_or_descpiption(value: &str, is_desc: bool) -> String {
    // All Unicode characters are valid, but backslashes, double quotes, and line feeds must be
    // escaped.
    let mut sanitized = String::with_capacity(value.as_bytes().len());

    let mut previous_backslash = false;
    for c in value.chars() {
        match c {
            // Any raw newlines get escaped, period.
            '\n' => sanitized.push_str("\\n"),
            // Any double quote we see gets escaped, but only for label values, not descriptions.
            '"' if !is_desc => {
                previous_backslash = false;
                sanitized.push_str("\\\"");
            }
            // If we see a backslash, we might be either seeing one that is being used to escape
            // something, or seeing one that has being escaped. If our last character was a
            // backslash, then we know this one has already been escaped, and we just emit the
            // escaped backslash.
            '\\' => {
                if previous_backslash {
                    // This backslash was preceded by another backslash, so we can safely emit an
                    // escaped backslash.
                    sanitized.push_str("\\\\");
                }

                // This may or may not be a backslash that is about to escape something else, so if
                // we toggle the value here: if it was false, then we're marking ourselves as having
                // seen a previous backslash (duh) or we just emitted an escaped backslash and now
                // we're clearing the flag.
                previous_backslash = !previous_backslash;
            }
            c => {
                // If we had a backslash in holding, and we're here, we know it wasn't escaping
                // something we care about, so it's on its own, and we emit an escaped backslash,
                // before emitting the actual character we're handling.
                if previous_backslash {
                    previous_backslash = false;
                    sanitized.push_str("\\\\");
                }
                sanitized.push(c);
            }
        }
    }

    // Handle any dangling backslash by writing it out in an escaped fashion.
    if previous_backslash {
        sanitized.push_str("\\\\");
    }

    sanitized
}

#[cfg(test)]
mod tests {
    use crate::common::*;
    use proptest::prelude::*;

    #[test]
    fn test_sanitize_metric_name_known_cases() {
        let cases = &[("*", "_"), ("\"", "_"), ("foo_bar", "foo_bar"), ("1foobar", "_foobar")];

        for (input, expected) in cases {
            let result = sanitize_metric_name(input);
            assert_eq!(expected, &result);
        }
    }

    #[test]
    fn test_sanitize_label_key_known_cases() {
        let cases = &[
            ("*", "_"),
            ("\"", "_"),
            (":", "_"),
            ("foo_bar", "foo_bar"),
            ("1foobar", "_foobar"),
            ("__foobar", "___foobar"),
        ];

        for (input, expected) in cases {
            let result = sanitize_label_key(input);
            assert_eq!(expected, &result);
        }
    }

    #[test]
    fn test_sanitize_label_value_known_cases() {
        let cases = &[
            ("*", "*"),
            ("\"", "\\\""),
            ("\\", "\\\\"),
            ("\\\\", "\\\\"),
            ("\n", "\\n"),
            ("foo_bar", "foo_bar"),
            ("1foobar", "1foobar"),
        ];

        for (input, expected) in cases {
            let result = sanitize_label_value(input);
            assert_eq!(expected, &result);
        }
    }

    #[test]
    fn test_sanitize_description_known_cases() {
        let cases = &[
            ("*", "*"),
            ("\"", "\""),
            ("\\", "\\\\"),
            ("\\\\", "\\\\"),
            ("\n", "\\n"),
            ("foo_bar", "foo_bar"),
            ("1foobar", "1foobar"),
        ];

        for (input, expected) in cases {
            let result = sanitize_description(input);
            assert_eq!(expected, &result);
        }
    }

    proptest! {
        #[test]
        fn test_sanitize_metric_name(input in "[\n\"\\\\]?.*[\n\"\\\\]?") {
            let result = sanitize_metric_name(&input);
            let as_chars = result.chars().collect::<Vec<_>>();

            if let Some(c) = as_chars.first() {
                assert_eq!(false, invalid_metric_name_start_character(*c),
                    "first character of metric name was not valid");
            }

            assert!(!as_chars.iter().any(|c| invalid_metric_name_character(*c)),
                "invalid character in metric name");
        }

        #[test]
        fn test_sanitize_label_key(input in "[\n\"\\\\:]?.*[\n\"\\\\:]?") {
            let result = sanitize_label_key(&input);
            let as_chars = result.chars().collect::<Vec<_>>();

            if let Some(c) = as_chars.first() {
                assert_eq!(false, invalid_label_key_start_character(*c),
                    "first character of label key was not valid");
            }

            // Label keys cannot begin with two underscores, as that format is reserved for internal
            // use.
            if as_chars.len() == 2 {
                assert!(!(as_chars[0] == '_' && as_chars[1] == '_'));
            } else if as_chars.len() == 3 {
                if as_chars[0] == '_' && as_chars[1] == '_' {
                    assert_eq!(as_chars[2], '_');
                }
            }

            assert!(!as_chars.iter().any(|c| invalid_label_key_character(*c)),
                "invalid character in label key");
        }

        #[test]
        fn test_sanitize_label_value(input in "[\n\"\\\\]?.*[\n\"\\\\]?") {
            let result = sanitize_label_value(&input);

            // If any raw newlines are still present, then we messed up.
            assert!(!result.contains('\n'), "raw/unescaped newlines present");

            // We specifically remove instances of "\\" because we only care about dangling backslashes.
            let delayered_backslashes = result.replace("\\\\", "");
            let as_chars = delayered_backslashes.chars().collect::<Vec<_>>();

            // If the first character is a double quote, then we messed up.
            assert!(as_chars.first().map(|c| *c != '"').unwrap_or(true),
                "first character cannot be a double quote: {}", result);

            // Now look for unescaped characters in the rest of the string, in a windowed fashion.
            let contained_unescaped_chars = as_chars.as_slice()
                .windows(2)
                .any(|s| {
                    let first = s[0];
                    let second = s[1];

                    match (first, second) {
                        // If there's a double quote, it has to have been preceded by an escaping
                        // backslash.
                        (c, '"') => c != '\\',
                        // If there's a backslash, it can only be in front of an 'n' for escaping
                        // newlines.
                        ('\\', c) => c != 'n',
                        // Everything else is valid.
                        _ => false,
                    }
                });
            assert!(!contained_unescaped_chars, "invalid or missing escape detected");
        }

        #[test]
        fn test_sanitize_description(input in "[\n\"\\\\]?.*[\n\"\\\\]?") {
            let result = sanitize_description(&input);

            // If any raw newlines are still present, then we messed up.
            assert!(!result.contains('\n'), "raw/unescaped newlines present");

            // We specifically remove instances of "\\" because we only care about dangling backslashes.
            let delayered_backslashes = result.replace("\\\\", "");
            let as_chars = delayered_backslashes.chars().collect::<Vec<_>>();

            // Now look for unescaped characters in the rest of the string, in a windowed fashion.
            let contained_unescaped_chars = as_chars.as_slice()
                .windows(2)
                .any(|s| {
                    let first = s[0];
                    let second = s[1];

                    match (first, second) {
                        // If there's a backslash, it can only be in front of an 'n' for escaping
                        // newlines.
                        ('\\', c) => c != 'n',
                        // Everything else is valid.
                        _ => false,
                    }
                });
            assert!(!contained_unescaped_chars, "invalid or missing escape detected");
        }
    }
}
