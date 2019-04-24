//! Records metrics in the Prometheus exposition format.
use std::time::SystemTime;
use hdrhistogram::Histogram;
use metrics_core::MetricsRecorder;
use metrics_util::{Quantile, parse_quantiles};

/// Records metrics in the Prometheus exposition format.
pub struct PrometheusRecorder {
    quantiles: Vec<Quantile>,
    output: String,
}

impl PrometheusRecorder {
    /// Creates a new [`PrometheusRecorder`] with a default set of quantiles.
    ///
    /// Configures the recorder with these default quantiles: 0.0, 0.5, 0.9, 0.95, 0.99, 0.999, and
    /// 1.0.  If you want to customize the quantiles used, you can call
    ///   [`PrometheusRecorder::with_quantiles`].
    pub fn new() -> Self {
        Self::with_quantiles(&[0.0, 0.5, 0.9, 0.95, 0.99, 0.999, 1.0])
    }

    /// Creates a new [`PrometheusRecorder`] with the given set of quantiles.
    pub fn with_quantiles(quantiles: &[f64]) -> Self {
        let actual_quantiles = parse_quantiles(quantiles);
        Self {
            quantiles: actual_quantiles,
            output: get_prom_expo_header(),
        }
    }
}

impl MetricsRecorder for PrometheusRecorder {
    fn record_counter<K: AsRef<str>>(&mut self, key: K, value: u64) {
        let label = key.as_ref().replace('.', "_");
        self.output.push_str("\n# TYPE ");
        self.output.push_str(label.as_str());
        self.output.push_str(" counter\n");
        self.output.push_str(label.as_str());
        self.output.push_str(" ");
        self.output.push_str(value.to_string().as_str());
        self.output.push_str("\n");
    }

    fn record_gauge<K: AsRef<str>>(&mut self, key: K, value: i64) {
        let label = key.as_ref().replace('.', "_");
        self.output.push_str("\n# TYPE ");
        self.output.push_str(label.as_str());
        self.output.push_str(" gauge\n");
        self.output.push_str(label.as_str());
        self.output.push_str(" ");
        self.output.push_str(value.to_string().as_str());
        self.output.push_str("\n");
    }

    fn record_histogram<K: AsRef<str>>(&mut self, key: K, values: &[u64]) {
        let mut sum = 0;
        let mut h = Histogram::<u64>::new(3).expect("failed to create histogram");
        for value in values {
            h.record(*value).expect("failed to record histogram value");
            sum += *value;
        }

        let label = key.as_ref().replace('.', "_");
        self.output.push_str("\n# TYPE ");
        self.output.push_str(label.as_str());
        self.output.push_str(" summary\n");

        for quantile in &self.quantiles {
            let value = h.value_at_quantile(quantile.value());
            self.output.push_str(label.as_str());
            self.output.push_str("{quantile=\"");
            self.output.push_str(quantile.value().to_string().as_str());
            self.output.push_str("\"} ");
            self.output.push_str(value.to_string().as_str());
            self.output.push_str("\n");
        }
        self.output.push_str(label.as_str());
        self.output.push_str("_sum ");
        self.output.push_str(sum.to_string().as_str());
        self.output.push_str("\n");
        self.output.push_str(label.as_str());
        self.output.push_str("_count ");
        self.output.push_str(values.len().to_string().as_str());
        self.output.push_str("\n");
    }
}

impl Clone for PrometheusRecorder {
    fn clone(&self) -> Self {
        Self {
            output: get_prom_expo_header(),
            quantiles: self.quantiles.clone(),
        }
    }
}

impl Into<String> for PrometheusRecorder {
    fn into(self) -> String {
        self.output
    }
}

fn get_prom_expo_header() -> String {
    let ts = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    format!("# metrics snapshot (ts={}) (prometheus exposition format)", ts)
}
