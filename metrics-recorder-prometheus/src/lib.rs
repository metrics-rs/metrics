//! Records metrics in the Prometheus exposition format.
#![deny(missing_docs)]
use hdrhistogram::Histogram;
use metrics_core::{Builder, Key, Label, Recorder};
use metrics_util::{parse_quantiles, Quantile};
use std::collections::HashMap;
use std::time::SystemTime;

/// Builder for [`PrometheusRecorder`].
pub struct PrometheusBuilder {
    quantiles: Vec<Quantile>,
}

impl PrometheusBuilder {
    /// Creates a new [`PrometheusBuilder`] with a default set of quantiles.
    ///
    /// Configures the recorder with these default quantiles: 0.0, 0.5, 0.9, 0.95, 0.99, 0.999, and
    /// 1.0.  If you want to customize the quantiles used, you can call
    ///   [`PrometheusBuilder::with_quantiles`].
    ///
    /// The configured quantiles are used when rendering any histograms.
    pub fn new() -> Self {
        Self::with_quantiles(&[0.0, 0.5, 0.9, 0.95, 0.99, 0.999, 1.0])
    }

    /// Creates a new [`PrometheusBuilder`] with the given set of quantiles.
    ///
    /// The configured quantiles are used when rendering any histograms.
    pub fn with_quantiles(quantiles: &[f64]) -> Self {
        let actual_quantiles = parse_quantiles(quantiles);

        Self {
            quantiles: actual_quantiles,
        }
    }
}

impl Builder for PrometheusBuilder {
    type Output = PrometheusRecorder;

    fn build(&self) -> Self::Output {
        PrometheusRecorder {
            quantiles: self.quantiles.clone(),
            histos: HashMap::new(),
            output: get_prom_expo_header(),
        }
    }
}

/// Records metrics in the Prometheus exposition format.
pub struct PrometheusRecorder {
    pub(crate) quantiles: Vec<Quantile>,
    pub(crate) histos: HashMap<Key, (u64, Histogram<u64>)>,
    pub(crate) output: String,
}

impl Recorder for PrometheusRecorder {
    fn record_counter(&mut self, key: Key, value: u64) {
        let (name, labels) = key_to_parts(key);
        let full_name = render_labeled_name(&name, &labels);
        self.output.push_str("\n# TYPE ");
        self.output.push_str(name.as_str());
        self.output.push_str(" counter\n");
        self.output.push_str(full_name.as_str());
        self.output.push_str(" ");
        self.output.push_str(value.to_string().as_str());
        self.output.push_str("\n");
    }

    fn record_gauge(&mut self, key: Key, value: i64) {
        let (name, labels) = key_to_parts(key);
        let full_name = render_labeled_name(&name, &labels);
        self.output.push_str("\n# TYPE ");
        self.output.push_str(name.as_str());
        self.output.push_str(" gauge\n");
        self.output.push_str(full_name.as_str());
        self.output.push_str(" ");
        self.output.push_str(value.to_string().as_str());
        self.output.push_str("\n");
    }

    fn record_histogram(&mut self, key: Key, values: &[u64]) {
        let entry = self.histos.entry(key).or_insert_with(|| {
            let h = Histogram::<u64>::new(3).expect("failed to create histogram");
            (0, h)
        });

        let (sum, h) = entry;
        for value in values {
            h.record(*value).expect("failed to record histogram value");
            *sum += *value;
        }
    }
}

impl From<PrometheusRecorder> for String {
    fn from(r: PrometheusRecorder) -> String {
        let mut output = r.output;

        for (key, sh) in r.histos {
            let (sum, hist) = sh;
            let (name, labels) = key_to_parts(key);
            output.push_str("\n# TYPE ");
            output.push_str(name.as_str());
            output.push_str(" summary\n");

            for quantile in &r.quantiles {
                let value = hist.value_at_quantile(quantile.value());
                let mut labels = labels.clone();
                labels.push(format!("quantile=\"{}\"", quantile.value()));
                let full_name = render_labeled_name(&name, &labels);
                output.push_str(full_name.as_str());
                output.push_str(" ");
                output.push_str(value.to_string().as_str());
                output.push_str("\n");
            }
            let sum_name = format!("{}_sum", name);
            let full_sum_name = render_labeled_name(&sum_name, &labels);
            output.push_str(full_sum_name.as_str());
            output.push_str(" ");
            output.push_str(sum.to_string().as_str());
            output.push_str("\n");
            let count_name = format!("{}_count", name);
            let full_count_name = render_labeled_name(&count_name, &labels);
            output.push_str(full_count_name.as_str());
            output.push_str(" ");
            output.push_str(hist.len().to_string().as_str());
            output.push_str("\n");
        }

        output
    }
}

fn key_to_parts(key: Key) -> (String, Vec<String>) {
    let (name, labels) = key.into_parts();
    let name = name.replace('.', "_");
    let labels = labels
        .into_iter()
        .map(Label::into_parts)
        .map(|(k, v)| format!("{}=\"{}\"", k, v))
        .collect();

    (name, labels)
}

fn render_labeled_name(name: &str, labels: &[String]) -> String {
    let mut output = name.to_string();
    if !labels.is_empty() {
        let joined = labels.join(",");
        output.push_str("{");
        output.push_str(&joined);
        output.push_str("}");
    }
    output
}

fn get_prom_expo_header() -> String {
    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    format!(
        "# metrics snapshot (ts={}) (prometheus exposition format)",
        ts
    )
}
