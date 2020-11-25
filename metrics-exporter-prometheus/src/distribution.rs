use std::{collections::HashMap, sync::Arc};

use crate::common::Matcher;

use hdrhistogram::Histogram as HdrHistogram;
use metrics_util::{Histogram, Quantile};

#[derive(Clone, Debug)]
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
    Summary(HdrHistogram<u64>, Arc<Vec<Quantile>>, u64),
}

impl Distribution {
    pub fn new_histogram(buckets: &[u64]) -> Option<Distribution> {
        let hist = Histogram::new(buckets)?;
        Some(Distribution::Histogram(hist))
    }

    pub fn new_summary(quantiles: Arc<Vec<Quantile>>) -> Option<Distribution> {
        let hist = HdrHistogram::new(3).ok()?;
        Some(Distribution::Summary(hist, quantiles, 0))
    }

    pub fn record_samples(&mut self, samples: &[u64]) {
        match self {
            Distribution::Histogram(hist) => hist.record_many(samples),
            Distribution::Summary(hist, _, sum) => {
                for sample in samples {
                    let _ = hist.record(*sample);
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
    buckets: Option<Vec<u64>>,
    bucket_overrides: Option<Vec<(Matcher, Vec<u64>)>>,
}

impl DistributionBuilder {
    pub fn new(
        quantiles: Vec<Quantile>,
        buckets: Option<Vec<u64>>,
        bucket_overrides: Option<HashMap<Matcher, Vec<u64>>>,
    ) -> DistributionBuilder {
        DistributionBuilder {
            quantiles: Arc::new(quantiles),
            buckets,
            bucket_overrides: bucket_overrides.map(|entries| {
                let mut matchers = entries.into_iter().collect::<Vec<_>>();
                matchers.sort();
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
