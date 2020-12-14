use sketches_ddsketch::{Config, DDSketch};

/// A quantile sketch with relative-error guarantees.
///
/// Based on [DDSketch](ddsketch), `Summary` provides quantiles over an arbitrary distribution of
/// floating-point numbers, including for negative numbers, using a space-efficient sketch that
/// provides relative-error guarantees, regardless of the absolute range between the smallest and
/// larger values.
///
/// `Summary` is similiar to [HDRHistogram](hdrhistogram) in practice, but supports an arbitrary
/// range of values, and supports floating-point numbers.
///
/// Numbers smaller than given `min_value` will be recognized as zeroes.
/// 
/// Memory usage for `Summary` should be nearly identical to `DDSketch` when only using positive
/// numbers, but as `Summary` supports negative numbers as well, memory usage could be as high as 2x
/// that of `DDSketch`.
///
/// [ddsketch]: https://arxiv.org/abs/1908.10693
/// [hdrhistogram]: https://docs.rs/hdrhistogram
#[derive(Clone)]
pub struct Summary {
    negative: DDSketch,
    positive: DDSketch,
    min_value: f64,
    zeroes: usize,
    min: Option<f64>,
    max: Option<f64>,
}

impl Summary {
    /// Creates a new [`Summary`].
    ///
    /// `alpha` represents the desired relative error foir this summary.  If `alpha` was 0.001, that
    /// would represent a desired relative error of 0.01%.  For example, if the true value at
    /// quantile q0 was 1, the estimated value at that quantile would be a value within 0.01% of the
    /// true value, or a value between 0.999 and 1.001.
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
        let bucket_split = max_buckets;
        let config = Config::new(alpha, bucket_split, min_value.abs());

        Summary {
            negative: DDSketch::new(config.clone()),
            positive: DDSketch::new(config),
            min_value: min_value.abs(),
            zeroes: 0,
            min: None,
            max: None,
        }
    }

    /// Creates a new [`Summary`] with default values.
    ///
    /// `alpha` is 0.0001, `max_buckets` is 32,768, and `min_value` is 1.0e-9.
    ///
    /// This will yield a summary that is roughly equivalent in memory usage to an HDRHistogram with
    /// 3 significant digits, and will support values down to a single nanosecond.
    pub fn with_defaults() -> Summary {
        Summary::new(0.0001, 32_768, 1.0e-9)
    }

    /// Adds a sample to the summary.
    pub fn add(&mut self, value: f64) {
        match self.min {
            None => self.min = Some(value),
            Some(ref mut min) => {
                if &value < min {
                    *min = value;
                }
            }
        }

        match self.max {
            None => self.max = Some(value),
            Some(ref mut max) => {
                if &value > max {
                    *max = value;
                }
            }
        }

        let vabs = value.abs();
        if vabs > 0.0 {
            if vabs <= self.min_value {
                self.zeroes += 1;
            } else {
                if value > 0.0 {
                    println!("padd");
                    self.positive.add(vabs);
                } else {
                    println!("nadd");
                    self.negative.add(vabs);
                }
            }
        }
    }

    /// Gets the estimated value at the given quantile.
    ///
    /// If the sketch is empty, or if the quantile is less than 0.0 or greater than 1.0, then the
    /// result will be `None`.
    pub fn quantile(&self, q: f64) -> Option<f64> {
        if q < 0.0 || q > 1.0 {
            return None;
        }

        let ncount = self.negative.count();
        let pcount = self.positive.count();
        let zcount = self.zeroes;
        let total = ncount + pcount + zcount;
        let rank = (q * total as f64).ceil() as usize;

        println!("qdiag: q={} ncount={} pcount={} zcount={} total={} rank={}",
            q, ncount, pcount, zcount, total, rank);

        if rank < ncount {
            // Quantile lands in the negative side.
            let nq = 1.0 - (rank as f64 / ncount as f64);
            self.negative
                .quantile(nq)
                .expect("quantile was already validated")
                .map(|v| -v)
        } else if rank > ncount && rank < (ncount + zcount) {
            // Quantile lands in the zero band.
            Some(0.0)
        } else {
            // Quantile lands in the positive side.
            let pq = (rank - (ncount + zcount)) as f64 / pcount as f64;
            self.positive
                .quantile(pq)
                .expect("quantile was already validated")
        }
    }

    /// Gets the minimum value this summary has seen so far.
    pub fn min(&self) -> Option<f64> {
        self.min
    }

    /// Gets the maximum value this summary has seen so far.
    pub fn max(&self) -> Option<f64> {
        self.max
    }

    /// Gets the number of samples in this summary.
    pub fn count(&self) -> usize {
        self.negative.count() + self.positive.count()
    }

    /// Estimized size of this summary, in bytes.
    pub fn size(&self) -> usize {
        std::mem::size_of::<Self>() + ((self.positive.length() + self.negative.length()) * 8)
    }
}
