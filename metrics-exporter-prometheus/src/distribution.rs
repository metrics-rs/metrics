use std::{collections::HashMap, sync::Arc};

use crate::common::Matcher;

use metrics_util::{Histogram, Quantile, Summary};

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
    Summary(Summary, Arc<Vec<Quantile>>, f64),
}

impl Distribution {
    pub fn new_histogram(buckets: &[f64]) -> Option<Distribution> {
        let hist = Histogram::new(buckets)?;
        Some(Distribution::Histogram(hist))
    }

    pub fn new_summary(quantiles: Arc<Vec<Quantile>>) -> Option<Distribution> {
        let summary = Summary::with_defaults();
        Some(Distribution::Summary(summary, quantiles, 0.0))
    }

    pub fn record_samples(&mut self, samples: &[f64]) {
        match self {
            Distribution::Histogram(hist) => hist.record_many(samples),
            Distribution::Summary(hist, _, sum) => {
                for sample in samples {
                    hist.add(*sample);
                    *sum += *sample;
                }
            }
        }
    }
}

/// Builds distributions for metric names based on a set of configured overrides.
#[derive(Debug)]
pub struct DistributionBuilder {
    quantiles: Arc<Vec<Quantile>>,
    buckets: Option<Vec<f64>>,
    bucket_overrides: Option<Vec<(Matcher, Vec<f64>)>>,
}

impl DistributionBuilder {
    pub fn new(
        quantiles: Vec<Quantile>,
        buckets: Option<Vec<f64>>,
        bucket_overrides: Option<HashMap<Matcher, Vec<f64>>>,
    ) -> DistributionBuilder {
        DistributionBuilder {
            quantiles: Arc::new(quantiles),
            buckets,
            bucket_overrides: bucket_overrides.map(|entries| {
                let mut matchers = entries.into_iter().collect::<Vec<_>>();
                matchers.sort_by(|a, b| a.0.cmp(&b.0));
                matchers
            }),
        }
    }

    pub fn get_distribution(&self, name: &str) -> Option<Distribution> {
        if let Some(ref overrides) = self.bucket_overrides {
            for (matcher, buckets) in overrides.iter() {
                if matcher.matches(name) {
                    return Distribution::new_histogram(buckets);
                }
            }
        }

        if let Some(ref buckets) = self.buckets {
            return Distribution::new_histogram(&buckets);
        }

        Distribution::new_summary(self.quantiles.clone())
    }

    pub fn get_distribution_type(&self, name: &str) -> &str {
        if self.buckets.is_some() {
            return "histogram";
        }

        if let Some(ref overrides) = self.bucket_overrides {
            for (matcher, _) in overrides.iter() {
                if matcher.matches(name) {
                    return "histogram";
                }
            }
        }

        "summary"
    }
}
