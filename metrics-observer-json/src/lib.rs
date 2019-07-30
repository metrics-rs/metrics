//! Observes metrics in JSON format.
//!
//! Metric scopes are used to provide the hierarchy of metrics.  As an example, for a
//! snapshot with two metrics — `server.msgs_received` and `server.msgs_sent` — we would
//! expect to see this output:
//!
//! ```c
//! {"server":{"msgs_received":42,"msgs_sent":13}}
//! ```
//!
//! If we added another metric — `configuration_reloads` — we would expect to see:
//!
//! ```c
//! {"configuration_reloads":2,"server":{"msgs_received":42,"msgs_sent":13}}
//! ```
//!
//! Metrics are sorted alphabetically.
//!
//! ## Histograms
//!
//! Histograms are rendered with a configurable set of quantiles that are provided when creating an
//! instance of `JsonBuilder`.  They are formatted using human-readable labels when displayed to
//! the user.  For example, 0.0 is rendered as "min", 1.0 as "max", and anything in between using
//! the common "pXXX" format i.e. a quantile of 0.5 or percentile of 50 would be p50, a quantile of
//! 0.999 or percentile of 99.9 would be p999, and so on.
//!
//! All histograms have the sample count of the histogram provided in the output.
//!
//! ```c
//! {"connect_time_count":15,"connect_time_min":1334,"connect_time_p50":1934,
//! "connect_time_p99":5330,"connect_time_max":139389}
//! ```
//!
#![deny(missing_docs)]
use hdrhistogram::Histogram;
use metrics_core::{Builder, Drain, Key, Label, Observer};
use metrics_util::{parse_quantiles, MetricsTree, Quantile};
use std::collections::HashMap;

/// Builder for [`JsonObserver`].
pub struct JsonBuilder {
    quantiles: Vec<Quantile>,
    pretty: bool,
}

impl JsonBuilder {
    /// Creates a new [`JsonBuilder`] with default values.
    pub fn new() -> Self {
        let quantiles = parse_quantiles(&[0.0, 0.5, 0.9, 0.95, 0.99, 0.999, 1.0]);

        Self {
            quantiles,
            pretty: false,
        }
    }

    /// Sets the quantiles to use when rendering histograms.
    ///
    /// Quantiles represent a scale of 0 to 1, where percentiles represent a scale of 1 to 100, so
    /// a quantile of 0.99 is the 99th percentile, and a quantile of 0.99 is the 99.9th percentile.
    ///
    /// By default, the quantiles will be set to: 0.0, 0.5, 0.9, 0.95, 0.99, 0.999, and 1.0.
    pub fn set_quantiles(mut self, quantiles: &[f64]) -> Self {
        self.quantiles = parse_quantiles(quantiles);
        self
    }

    /// Sets whether or not to render the JSON as "pretty."
    ///
    /// Pretty JSON refers to the formatting and identation, where different fields are on
    /// different lines, and depending on their depth from the root object, are indented.
    ///
    /// By default, pretty mode is not enabled.
    pub fn set_pretty_json(mut self, pretty: bool) -> Self {
        self.pretty = pretty;
        self
    }
}

impl Builder for JsonBuilder {
    type Output = JsonObserver;

    fn build(&self) -> Self::Output {
        JsonObserver {
            quantiles: self.quantiles.clone(),
            pretty: self.pretty,
            tree: MetricsTree::default(),
            histos: HashMap::new(),
        }
    }
}

impl Default for JsonBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Observes metrics in JSON format.
pub struct JsonObserver {
    pub(crate) quantiles: Vec<Quantile>,
    pub(crate) pretty: bool,
    pub(crate) tree: MetricsTree,
    pub(crate) histos: HashMap<Key, Histogram<u64>>,
}

impl Observer for JsonObserver {
    fn observe_counter(&mut self, key: Key, value: u64) {
        let (levels, name) = key_to_parts(key);
        self.tree.insert_value(levels, name, value);
    }

    fn observe_gauge(&mut self, key: Key, value: i64) {
        let (levels, name) = key_to_parts(key);
        self.tree.insert_value(levels, name, value);
    }

    fn observe_histogram(&mut self, key: Key, values: &[u64]) {
        let entry = self
            .histos
            .entry(key)
            .or_insert_with(|| Histogram::<u64>::new(3).expect("failed to create histogram"));

        for value in values {
            entry
                .record(*value)
                .expect("failed to observe histogram value");
        }
    }
}

impl Drain<String> for JsonObserver {
    fn drain(&mut self) -> String {
        for (key, h) in self.histos.drain() {
            let (levels, name) = key_to_parts(key);
            let values = hist_to_values(name, h.clone(), &self.quantiles);
            self.tree.insert_values(levels, values);
        }

        let result = if self.pretty {
            serde_json::to_string_pretty(&self.tree)
        } else {
            serde_json::to_string(&self.tree)
        };
        let rendered = result.expect("failed to render json output");
        self.tree.clear();
        rendered
    }
}

fn key_to_parts(key: Key) -> (Vec<String>, String) {
    let (name, labels) = key.into_parts();
    let mut parts = name.split('.').map(ToOwned::to_owned).collect::<Vec<_>>();
    let name = parts.pop().expect("name didn't have a single part");

    let labels = labels
        .into_iter()
        .map(Label::into_parts)
        .map(|(k, v)| format!("{}=\"{}\"", k, v))
        .collect::<Vec<_>>()
        .join(",");
    let label = if labels.is_empty() {
        String::new()
    } else {
        format!("{{{}}}", labels)
    };

    let fname = format!("{}{}", name, label);

    (parts, fname)
}

fn hist_to_values(
    name: String,
    hist: Histogram<u64>,
    quantiles: &[Quantile],
) -> Vec<(String, u64)> {
    let mut values = Vec::new();

    values.push((format!("{} count", name), hist.len()));
    for quantile in quantiles {
        let value = hist.value_at_quantile(quantile.value());
        values.push((format!("{} {}", name, quantile.label()), value));
    }

    values
}
