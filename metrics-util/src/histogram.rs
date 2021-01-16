//! Helper functions and types related to histogram data.

/// A bucketed histogram.
///
/// This histogram tracks the number of samples that fall into pre-defined buckets,
/// rather than exposing any sort of quantiles.
///
/// This type is most useful with systems that prefer bucketed data, such as Prometheus'
/// histogram type, as opposed to its summary type, which deals with quantiles.
#[derive(Debug, Clone)]
pub struct Histogram {
    count: u64,
    bounds: Vec<f64>,
    buckets: Vec<u64>,
    sum: f64,
}

impl Histogram {
    /// Creates a new `Histogram`.
    ///
    /// If `bounds` is empty, returns `None`.
    pub fn new(bounds: &[f64]) -> Option<Histogram> {
        if bounds.is_empty() {
            return None;
        }

        let buckets = vec![0u64; bounds.len()];

        Some(Histogram {
            count: 0,
            bounds: Vec::from(bounds),
            buckets,
            sum: 0.0,
        })
    }

    /// Gets the sum of all samples.
    pub fn sum(&self) -> f64 {
        self.sum
    }

    /// Gets the sample count.
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Gets the buckets.
    ///
    /// Buckets are tuples, where the first element is the bucket limit itself, and the second
    /// element is the count of samples in that bucket.
    pub fn buckets(&self) -> Vec<(f64, u64)> {
        self.bounds
            .iter()
            .cloned()
            .zip(self.buckets.iter().cloned())
            .collect()
    }

    /// Records a single sample.
    pub fn record(&mut self, sample: f64) {
        self.sum += sample;
        self.count += 1;

        // Add the sample to every bucket where the value is less than the bound.
        for (idx, bucket) in self.bounds.iter().enumerate() {
            if sample <= *bucket {
                self.buckets[idx] += 1;
            }
        }
    }

    /// Records multiple samples.
    pub fn record_many<'a, S>(&mut self, samples: S)
    where
        S: IntoIterator<Item = &'a f64> + 'a,
    {
        let mut bucketed = vec![0u64; self.buckets.len()];

        let mut sum = 0.0;
        let mut count = 0;
        for sample in samples.into_iter() {
            sum += *sample;
            count += 1;

            for (idx, bucket) in self.bounds.iter().enumerate() {
                if sample <= bucket {
                    bucketed[idx] += 1;
                    break;
                }
            }
        }

        // Add each bucket to the next bucket to satisfy the "less than or equal to"
        // behavior of the buckets.
        if bucketed.len() >= 2 {
            for idx in 0..(bucketed.len() - 1) {
                bucketed[idx + 1] += bucketed[idx];
            }
        }

        // Merge our temporary buckets to our main buckets.
        for (idx, local) in bucketed.iter().enumerate() {
            self.buckets[idx] += local;
        }
        self.sum += sum;
        self.count += count;
    }
}

#[cfg(test)]
mod tests {
    use super::Histogram;

    #[test]
    fn test_histogram() {
        // No buckets, can't do shit.
        let histogram = Histogram::new(&[]);
        assert!(histogram.is_none());

        let buckets = &[10.0, 25.0, 100.0];
        let values = vec![3.0, 2.0, 6.0, 12.0, 56.0, 82.0, 202.0, 100.0, 29.0];

        let mut histogram = Histogram::new(buckets).expect("histogram should have been created");

        histogram.record_many(&values);
        histogram.record(89.0);

        let result = histogram.buckets();
        assert_eq!(result.len(), 3);

        let (_, first) = result[0];
        assert_eq!(first, 3);
        let (_, second) = result[1];
        assert_eq!(second, 4);
        let (_, third) = result[2];
        assert_eq!(third, 9);

        assert_eq!(histogram.count(), values.len() as u64 + 1);
        assert_eq!(histogram.sum(), 581.0);
    }
}
