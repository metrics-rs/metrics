use sketches_ddsketch::{Config, DDSketch};

/// A quantile sketch with relative-error guarantees.
///
/// Based on [DDSketch][ddsketch], `Summary` provides quantiles over an arbitrary distribution of
/// floating-point numbers, including for negative numbers, using a space-efficient sketch that
/// provides relative-error guarantees, regardless of the absolute range between the smallest and
/// larger values.
///
/// `Summary` is similiar to [HDRHistogram][hdrhistogram] in practice, but supports an arbitrary
/// range of values, and supports floating-point numbers.
///
/// Numbers with an absolute value smaller than given `min_value` will be recognized as zeroes.
///
/// Memory usage for `Summary` should be nearly identical to `DDSketch`.
/// [`Summary::estimated_size`] provides a rough estimate of summary size based on the current
/// values that have been added to it.
///
/// As mentioned above, this sketch provides relative-error guarantees across quantiles falling
/// within 0 <= q <= 1, but trades some accuracy at the lowest quantiles as part of the collapsing
/// scheme that allows for automatically handling arbitrary ranges of values, even when the
/// maximum number of bins has been allocated.  Typically, q=0.05 and below is where this error will
/// be noticed, if present.
///
/// For cases when all values are positive, you can simply use [`Summary::min`] in lieu of checking
/// these quantiles, as the minimum value will be closer to the true value.  For cases when values
/// range from negative to positive, the aforementioned collapsing will perturb the estimated true
/// value for quantiles that conceptually fall within this collapsed band.
///
/// For example, for a distribution that spans from -25 to 75, we would intuitively expect q=0 to be
/// -25, q=0.25 to be 0, q=0.5 to be 25, and so on.  Internally, negative numbers and positive
/// numbers are handled in two separate containers.  Based on this example, one container would
/// handle -25 to 0, and another would handle the 0 to 75 range.  As the containers are mapped "back
/// to back", q=0.25 for this hypothetical summary would actually be q=0 within the negative
/// container, which may return an estimated true value that exceeds the relative error guarantees.
///
/// Of course, as these problems are related to the estimation aspect of this data structure, users
/// can allow the summary to allocate more bins to compensate for these edge cases, if desired.
///
/// [ddsketch]: https://arxiv.org/abs/1908.10693
/// [hdrhistogram]: https://docs.rs/hdrhistogram
#[derive(Clone)]
pub struct Summary {
    negative: DDSketch,
    positive: DDSketch,
    min_value: f64,
    zeroes: usize,
    min: f64,
    max: f64,
}

impl Summary {
    /// Creates a new [`Summary`].
    ///
    /// `alpha` represents the desired relative error for this summary.  If `alpha` was 0.0001, that
    /// would represent a desired relative error of 0.01%.  For example, if the true value at
    /// quantile q0 was 1, the estimated value at that quantile would be a value within 0.01% of the
    /// true value, or a value between 0.9999 and 1.0001.
    ///
    /// `max_buckets` controls how many subbuckets are created, which directly influences memory usage.
    /// Each bucket "costs" eight bytes, so a summary with 2048 buckets would consume a maximum of
    /// around 16 KiB.  Depending on how many samples have been added to the summary, the number of
    /// subbuckets allocated may be far below `max_buckets`, and the summary will allocate more as
    /// needed to fulfill the relative error guarantee.
    ///
    /// `min_value` controls the smallest value that will be recognized distinctly from zero.  Said
    /// another way, any value between `-min_value` and `min_value` will be counted as zero.
    pub fn new(alpha: f64, max_buckets: u32, min_value: f64) -> Summary {
        let config = Config::new(alpha, max_buckets, min_value.abs());

        Summary {
            negative: DDSketch::new(config),
            positive: DDSketch::new(config),
            min_value: min_value.abs(),
            zeroes: 0,
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
        }
    }

    /// Creates a new [`Summary`] with default values.
    ///
    /// `alpha` is 0.0001, `max_buckets` is 32,768, and `min_value` is 1.0e-9.
    ///
    /// This will yield a summary that is roughly equivalent in memory usage to an HDRHistogram with
    /// 3 significant digits, and will support values down to a single nanosecond.
    ///
    /// In practice, when using only positive values, maximum memory usage can be expected to hover
    /// around 200KiB, while usage of negative values can lead to an average maximum size of around
    /// 400KiB.
    pub fn with_defaults() -> Summary {
        Summary::new(0.0001, 32_768, 1.0e-9)
    }

    /// Adds a sample to the summary.
    ///
    /// If the absolute value of `value` is smaller than given `min_value`, it will be added as a zero.
    pub fn add(&mut self, value: f64) {
        if value.is_infinite() {
            return;
        }

        if value < self.min {
            self.min = value;
        }

        if value > self.max {
            self.max = value;
        }

        if value > self.min_value {
            self.positive.add(value);
        } else if value < -self.min_value {
            self.negative.add(-value);
        } else {
            self.zeroes += 1;
        }
    }

    /// Gets the estimated value at the given quantile.
    ///
    /// If the sketch is empty, or if the quantile is less than 0.0 or greater than 1.0, then the
    /// result will be `None`.
    ///
    /// While `q` can be either 0.0 or 1.0, callers should prefer to use [`Summary::min`] and
    /// [`Summary::max`] as the values will be the true values, and not an estimation.
    pub fn quantile(&self, q: f64) -> Option<f64> {
        if q < 0.0 || q > 1.0 || self.count() == 0 {
            return None;
        }

        let ncount = self.negative.count();
        let pcount = self.positive.count();
        let zcount = self.zeroes;
        let total = ncount + pcount + zcount;
        let rank = (q * (total - 1) as f64) as usize;

        if rank < ncount {
            // Quantile lands in the negative side.
            let nq = 1.0 - (rank as f64 / ncount as f64);
            self.negative
                .quantile(nq)
                .expect("quantile should be valid at this point")
                .map(|v| -v)
        } else if rank >= ncount && rank < (ncount + zcount) {
            // Quantile lands in the zero band.
            Some(0.0)
        } else {
            // Quantile lands in the positive side.
            let pq = (rank - (ncount + zcount)) as f64 / pcount as f64;
            self.positive
                .quantile(pq)
                .expect("quantile should be valid at this point")
        }
    }

    /// Gets the minimum value this summary has seen so far.
    pub fn min(&self) -> f64 {
        self.min
    }

    /// Gets the maximum value this summary has seen so far.
    pub fn max(&self) -> f64 {
        self.max
    }

    /// Whether or not this summary is empty.
    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }

    /// Gets the number of samples in this summary.
    pub fn count(&self) -> usize {
        self.negative.count() + self.positive.count() + self.zeroes
    }

    /// Gets the number of samples in this summary by zeroes, negative, and positive counts.
    pub fn detailed_count(&self) -> (usize, usize, usize) {
        (self.zeroes, self.negative.count(), self.positive.count())
    }

    /// Gets the estimized size of this summary, in bytes.
    ///
    /// In practice, this value should be very close to the actual size, but will not be entirely
    /// precise.
    pub fn estimated_size(&self) -> usize {
        std::mem::size_of::<Self>() + ((self.positive.length() + self.negative.length()) * 8)
    }
}

#[cfg(test)]
mod tests {
    use super::Summary;

    use quickcheck_macros::quickcheck;

    // Need this, because without the relative_eq/abs_diff_eq imports, we get weird IDE errors.
    #[allow(unused_imports)]
    use approx::{abs_diff_eq, assert_abs_diff_eq, assert_relative_eq, relative_eq};

    use ndarray::{Array1, Axis};
    use ndarray_stats::{interpolate::Linear, QuantileExt};
    use noisy_float::types::n64;
    use ordered_float::NotNan;
    use rand::{distributions::Distribution, thread_rng};
    use rand_distr::Uniform;

    #[test]
    fn test_basics() {
        let mut summary = Summary::with_defaults();
        assert!(summary.is_empty());

        // Stretch the legs with a single value.
        summary.add(-420.42);
        assert_eq!(summary.count(), 1);
        assert_relative_eq!(summary.min(), -420.42);
        assert_relative_eq!(summary.max(), -420.42);
        assert_abs_diff_eq!(summary.quantile(0.1).expect("value should exist"), -420.42);
        assert_abs_diff_eq!(summary.quantile(0.5).expect("value should exist"), -420.42);
        assert_abs_diff_eq!(summary.quantile(0.99).expect("value should exist"), -420.42);

        summary.add(420.42);
        assert_eq!(summary.count(), 2);
        assert_relative_eq!(summary.min(), -420.42);
        assert_relative_eq!(summary.max(), 420.42);
        assert_abs_diff_eq!(summary.quantile(0.49).expect("value should exist"), -420.42);

        summary.add(42.42);
        assert_eq!(summary.count(), 3);
        assert_relative_eq!(summary.min(), -420.42);
        assert_relative_eq!(summary.max(), 420.42);
        assert_abs_diff_eq!(
            summary.quantile(0.4999999999).expect("value should exist"),
            -420.42
        );
        assert_abs_diff_eq!(summary.quantile(0.5).expect("value should exist"), 42.42);
        assert_abs_diff_eq!(
            summary.quantile(0.9999999999).expect("value should exist"),
            42.42
        );
    }

    #[test]
    fn test_positive_uniform() {
        let alpha = 0.0001;
        let max_bins = 32_768;
        let min_value = 1.0e-9;

        let mut rng = thread_rng();
        let dist = Uniform::new(0.0, 100.0);

        let mut summary = Summary::new(alpha, max_bins, min_value);
        let mut uniform = Vec::new();
        for _ in 0..100_000 {
            let value = dist.sample(&mut rng);
            uniform.push(NotNan::new(value).unwrap());
            summary.add(value);
        }

        uniform.sort();
        let mut true_histogram = Array1::from(uniform);

        let quantiles = &[0.25, 0.5, 0.75, 0.99];
        for quantile in quantiles {
            let aval_raw = true_histogram
                .quantile_axis_mut(Axis(0), n64(*quantile), &Linear)
                .expect("quantile should be in range");
            let aval = aval_raw
                .get(())
                .expect("quantile value should be present")
                .into_inner();
            let sval = summary
                .quantile(*quantile)
                .expect("quantile value should be present");

            // Multiply the true value by α, and double it to account from the -α/α swing.
            let distance = (aval * alpha) * 2.0;

            assert_relative_eq!(aval, sval, max_relative = distance);
        }
    }

    #[test]
    fn test_negative_positive_uniform() {
        let alpha = 0.0001;
        let max_bins = 65_536;
        let min_value = 1.0e-9;

        let mut rng = thread_rng();
        let dist = Uniform::new(-100.0, 100.0);

        let mut summary = Summary::new(alpha, max_bins, min_value);
        let mut uniform = Vec::new();
        for _ in 0..100_000 {
            let value = dist.sample(&mut rng);
            uniform.push(NotNan::new(value).unwrap());
            summary.add(value);
        }

        uniform.sort();
        let mut true_histogram = Array1::from(uniform);

        // We explicitly skirt q=0.5 here to avoid the edge case quantiles as best as possible
        // while asserting tightly to our relative error bound for everything else.
        let quantiles = &[0.25, 0.47, 0.75, 0.99];
        for quantile in quantiles {
            let aval_raw = true_histogram
                .quantile_axis_mut(Axis(0), n64(*quantile), &Linear)
                .expect("quantile should be in range");
            let aval = aval_raw
                .get(())
                .expect("quantile value should be present")
                .into_inner();
            let sval = summary
                .quantile(*quantile)
                .expect("quantile value should be present");

            // Multiply the true value by α, and quadruple it to account from the -α/α swing,
            // but also to account for the values sitting at the edge case quantiles.
            let distance = (aval.abs() * alpha) * 2.0;

            assert_relative_eq!(aval, sval, max_relative = distance);
        }
    }

    #[test]
    fn test_zeroes() {
        let mut summary = Summary::with_defaults();
        summary.add(0.0);
        assert_eq!(summary.quantile(0.5), Some(0.0));
    }

    #[test]
    fn test_infinities() {
        let mut summary = Summary::with_defaults();
        summary.add(f64::INFINITY);
        assert_eq!(summary.quantile(0.5), None);
        summary.add(f64::NEG_INFINITY);
        assert_eq!(summary.quantile(0.5), None);
    }

    #[quickcheck]
    fn quantile_validity(inputs: Vec<f64>) -> bool {
        let mut had_non_inf = false;

        let mut summary = Summary::with_defaults();
        for input in &inputs {
            if !input.is_infinite() {
                had_non_inf = true;
            }
            summary.add(*input);
        }

        let qs = &[0.0, 0.5, 0.9, 0.95, 0.99, 0.999, 1.0];
        for q in qs {
            let result = summary.quantile(*q);
            if had_non_inf {
                assert!(result.is_some());
            } else {
                assert!(result.is_none());
            }
        }

        true
    }
}
