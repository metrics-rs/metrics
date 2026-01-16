//! Helpers for rendering metrics in the Prometheus exposition format.

use metrics::Unit;

use crate::common::LabelSet;

/// Writes a help (description) line in the Prometheus [exposition format].
///
/// [exposition format]: https://github.com/prometheus/docs/blob/main/content/docs/instrumenting/exposition_formats.md#text-format-details
pub fn write_help_line(
    buffer: &mut String,
    name: &str,
    unit: Option<Unit>,
    suffix: Option<&'static str>,
    desc: &str,
) {
    buffer.push_str("# HELP ");
    add_metric_name(buffer, name, unit, suffix);
    buffer.push(' ');
    let desc = sanitize_description(desc);
    buffer.push_str(&desc);
    buffer.push('\n');
}

/// Writes a metric type line in the Prometheus [exposition format].
///
/// [exposition format]: https://github.com/prometheus/docs/blob/main/content/docs/instrumenting/exposition_formats.md#text-format-details
pub fn write_type_line(
    buffer: &mut String,
    name: &str,
    unit: Option<Unit>,
    suffix: Option<&'static str>,
    metric_type: &str,
) {
    buffer.push_str("# TYPE ");
    add_metric_name(buffer, name, unit, suffix);
    buffer.push(' ');
    buffer.push_str(metric_type);
    buffer.push('\n');
}

/// Writes a metric in the Prometheus [exposition format].
///
/// When `suffix` is specified, it is appended to the `name`, which is useful for writing summary
/// statistics, such as the sum or total of an aggregated histogram or aggregated summary.  Likewise,
/// `additional_label` would typically be used to specify a data type-specific label, such as `le` for
/// for aggregated histograms, or `quantile` for aggregated summaries.
///
/// [exposition format]: https://github.com/prometheus/docs/blob/main/content/docs/instrumenting/exposition_formats.md#text-format-details
pub fn write_metric_line<T, T2>(
    buffer: &mut String,
    name: &str,
    suffix: Option<&'static str>,
    labels: &LabelSet,
    additional_label: Option<(&'static str, T)>,
    value: T2,
    unit: Option<Unit>,
) where
    T: std::fmt::Display,
    T2: std::fmt::Display,
{
    add_metric_name(buffer, name, unit, suffix);

    if !labels.is_empty() || additional_label.is_some() {
        buffer.push('{');

        let mut first = true;
        for label in labels.to_strings() {
            if first {
                first = false;
            } else {
                buffer.push(',');
            }
            buffer.push_str(&label);
        }

        if let Some((name, value)) = additional_label {
            if !first {
                buffer.push(',');
            }
            buffer.push_str(name);
            buffer.push_str("=\"");
            buffer.push_str(value.to_string().as_str());
            buffer.push('"');
        }

        buffer.push('}');
    }

    buffer.push(' ');
    buffer.push_str(value.to_string().as_str());
    buffer.push('\n');
}

fn add_metric_name(
    buffer: &mut String,
    name: &str,
    unit: Option<Unit>,
    suffix: Option<&'static str>,
) {
    buffer.push_str(name);
    if let Some(unit) = unit {
        add_unit_if_missing(buffer, unit);
    }
    if let Some(suffix) = suffix {
        add_suffix_if_missing(buffer, suffix);
    }
}

/// Adds a suffix to the metric name if it is not already in the name.
fn add_suffix_if_missing(buffer: &mut String, suffix: &str) {
    if !buffer.ends_with(suffix) {
        buffer.push('_');
        buffer.push_str(suffix);
    }
}

/// Adds a unit to the metric name if it is not already in the name.
/// If the metric ends with a known suffix, we try to insert the unit before the suffix.
/// Otherwise, we append the unit to the end of the metric name.
fn add_unit_if_missing(buffer: &mut String, unit: Unit) {
    const KNOWN_SUFFIXES: [&str; 4] = ["_sum", "_count", "_bucket", "_total"];

    let unit = match unit {
        Unit::Count => {
            // For count, we don't suffix the unit.
            return;
        }
        Unit::Percent => "ratio",
        unit => unit.as_str(),
    };

    let mut handled = false;
    for suffix in KNOWN_SUFFIXES {
        if buffer.ends_with(suffix) {
            let suffix_pos = buffer.len() - suffix.len();
            // Check if name before suffix already has the unit
            if !&buffer[..suffix_pos].ends_with(unit) {
                buffer.insert(suffix_pos, '_');
                buffer.insert_str(suffix_pos + 1, unit);
            }
            handled = true;
            break;
        }
    }
    if !handled && !buffer.ends_with(unit) {
        buffer.push('_');
        buffer.push_str(unit);
    }
}

/// Sanitizes a metric name to be valid under the Prometheus [data model].
///
/// [data model]: https://prometheus.io/docs/concepts/data_model/#metric-names-and-labels
pub fn sanitize_metric_name(name: &str) -> String {
    // The first character must be [a-zA-Z_:], and all subsequent characters must be [a-zA-Z0-9_:].
    name.chars()
        .enumerate()
        .map(|(i, c)| {
            if i == 0 && valid_metric_name_start_character(c)
                || i != 0 && valid_metric_name_character(c)
            {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Sanitizes a label key to be valid under the Prometheus [data model].
///
/// [data model]: https://prometheus.io/docs/concepts/data_model/#metric-names-and-labels
pub fn sanitize_label_key(key: &str) -> String {
    // The first character must be [a-zA-Z_], and all subsequent characters must be [a-zA-Z0-9_].
    key.chars()
        .enumerate()
        .map(|(i, c)| {
            if i == 0 && valid_label_key_start_character(c)
                || i != 0 && valid_label_key_character(c)
            {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Sanitizes a label value to be valid under the Prometheus [data model].
///
/// [data model]: https://prometheus.io/docs/concepts/data_model/#metric-names-and-labels
pub fn sanitize_label_value(value: &str) -> String {
    sanitize_label_value_or_description(value, false)
}

/// Sanitizes a metric description to be valid under the Prometheus [exposition format].
///
/// [exposition format]: https://github.com/prometheus/docs/blob/main/content/docs/instrumenting/exposition_formats.md#text-format-details
pub fn sanitize_description(value: &str) -> String {
    sanitize_label_value_or_description(value, true)
}

fn sanitize_label_value_or_description(value: &str, is_desc: bool) -> String {
    // All Unicode characters are valid, but backslashes, double quotes, and line feeds must be
    // escaped.
    let mut sanitized = String::with_capacity(value.len());

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

#[inline]
fn valid_metric_name_start_character(c: char) -> bool {
    // Essentially, needs to match the regex pattern of [a-zA-Z_:].
    c.is_ascii_alphabetic() || c == '_' || c == ':'
}

#[inline]
fn valid_metric_name_character(c: char) -> bool {
    // Essentially, needs to match the regex pattern of [a-zA-Z0-9_:].
    c.is_ascii_alphanumeric() || c == '_' || c == ':'
}

#[inline]
fn valid_label_key_start_character(c: char) -> bool {
    // Essentially, needs to match the regex pattern of [a-zA-Z_].
    c.is_ascii_alphabetic() || c == '_'
}

#[inline]
fn valid_label_key_character(c: char) -> bool {
    // Essentially, needs to match the regex pattern of [a-zA-Z0-9_].
    c.is_ascii_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use crate::formatting::{
        sanitize_description, sanitize_label_key, sanitize_label_value, sanitize_metric_name,
        valid_label_key_character, valid_label_key_start_character, valid_metric_name_character,
        valid_metric_name_start_character,
    };
    use proptest::prelude::*;

    #[test]
    fn test_sanitize_metric_name_known_cases() {
        let cases = &[
            ("*", "_"),
            ("\"", "_"),
            ("foo_bar", "foo_bar"),
            ("foo1_bar", "foo1_bar"),
            ("1foobar", "_foobar"),
            ("foo1:bar2", "foo1:bar2"),
            ("123", "_23"),
        ];

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
            ("__foobar", "__foobar"),
            ("foo1bar2", "foo1bar2"),
            ("123", "_23"),
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
                assert!(valid_metric_name_start_character(*c),
                    "first character of metric name was not valid");
            }

            assert!(as_chars.iter().all(|c| valid_metric_name_character(*c)),
                "invalid character in metric name");
        }

        #[test]
        fn test_sanitize_label_key(input in "[\n\"\\\\:]?.*[\n\"\\\\:]?") {
            let result = sanitize_label_key(&input);
            let as_chars = result.chars().collect::<Vec<_>>();

            if let Some(c) = as_chars.first() {
                assert!(valid_label_key_start_character(*c),
                    "first character of label key was not valid");
            }

            // Label keys cannot begin with two underscores, as that format is reserved for internal
            // use.
            //
            // TODO: More closely examine how official Prometheus client libraries handle label key sanitization
            // and follow whatever they do, so it's not actually clear if transforming `__foo` to `___foo` would
            // be valid, given that it still technically starts with two underscores.
            /*if as_chars.len() == 2 {
                assert!(!(as_chars[0] == '_' && as_chars[1] == '_'));
            } else if as_chars.len() == 3 {
                if as_chars[0] == '_' && as_chars[1] == '_' {
                    assert_eq!(as_chars[2], '_');
                }
            }*/

            assert!(as_chars.iter().all(|c| valid_label_key_character(*c)),
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
            assert!(as_chars.first().map_or(true, |c| *c != '"'),
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
