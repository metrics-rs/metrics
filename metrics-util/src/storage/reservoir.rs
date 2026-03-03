//! An atomic sampling reservoir.

use metrics::atomics::AtomicU64;
use std::{
    cell::UnsafeCell,
    sync::{
        atomic::{
            AtomicBool, AtomicUsize,
            Ordering::{AcqRel, Acquire, Relaxed, Release},
        },
        Mutex,
    },
};

use rand::{rngs::OsRng, Rng, SeedableRng};
use rand_xoshiro::Xoshiro256StarStar;

thread_local! {
    static FAST_RNG: UnsafeCell<Xoshiro256StarStar> = {
        UnsafeCell::new(Xoshiro256StarStar::try_from_rng(&mut OsRng).unwrap())
    };
}

fn fastrand(upper: usize) -> usize {
    FAST_RNG.with(|rng| {
        // SAFETY: We know it's safe to take a mutable reference since we're getting a pointer to a thread-local value,
        // and the reference never outlives the closure executing on this thread.
        let rng = unsafe { &mut *rng.get() };
        rng.random_range(0..upper)
    })
}

struct Reservoir {
    values: Box<[AtomicU64]>,
    count: AtomicUsize,
    unsampled_sum: AtomicU64,
}

impl Reservoir {
    fn with_capacity(capacity: usize) -> Self {
        let mut values = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            values.push(AtomicU64::new(0));
        }

        Self {
            values: values.into_boxed_slice(),
            count: AtomicUsize::new(0),
            unsampled_sum: AtomicU64::new(0.0f64.to_bits()),
        }
    }

    fn push(&self, value: f64) {
        let idx = self.count.fetch_add(1, Relaxed);
        if idx < self.values.len() {
            self.values[idx].store(value.to_bits(), Relaxed);
        } else {
            let maybe_idx = fastrand(idx);
            if maybe_idx < self.values.len() {
                self.values[maybe_idx].store(value.to_bits(), Relaxed);
            }
        }

        loop {
            let result = self.unsampled_sum.fetch_update(Relaxed, Relaxed, |curr| {
                Some((f64::from_bits(curr) + value).to_bits())
            });
            if result.is_ok() {
                break;
            }
        }
    }

    fn drain(&self) -> Drain<'_> {
        let unsampled_len = self.count.load(Relaxed);
        let len = if unsampled_len > self.values.len() { self.values.len() } else { unsampled_len };
        let unsampled_sum = f64::from_bits(self.unsampled_sum.load(Relaxed));
        Drain { reservoir: self, unsampled_len, len, idx: 0, unsampled_sum }
    }
}

/// A draining iterator over the samples in a reservoir.
pub struct Drain<'a> {
    reservoir: &'a Reservoir,
    unsampled_len: usize,
    len: usize,
    idx: usize,
    unsampled_sum: f64,
}

impl<'a> Drain<'a> {
    /// Returns the total number of samples pushed into the reservoir,
    /// including those that were dropped by the sampling algorithm.
    pub fn unsampled_len(&self) -> usize {
        self.unsampled_len
    }

    /// Returns the sample rate of the reservoir that produced this iterator.
    ///
    /// The sample rate is the ratio of the number of samples pushed into the reservoir to the number of samples held in
    /// the reservoir. When the reservoir has not been filled, the sample rate is 1.0. When more samples have been
    /// pushed into the reservoir than its overall capacity, the sample rate is `size / count`, where `size` is the
    /// reservoir's capacity and `count` is the number of samples pushed into the reservoir.
    ///
    /// For example, if the reservoir holds 1,000 samples, and 100,000 values were pushed into the reservoir, the sample
    /// rate would be 0.01 (100,000 / 1,000).
    pub fn sample_rate(&self) -> f64 {
        if self.unsampled_len == self.len {
            1.0
        } else {
            self.len as f64 / self.unsampled_len as f64
        }
    }

    /// Returns the sum of all samples pushed into the reservoir,
    /// including those that were dropped by the sampling algorithm.
    pub fn unsampled_sum(&self) -> f64 {
        self.unsampled_sum
    }
}

impl<'a> Iterator for Drain<'a> {
    type Item = f64;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx < self.len {
            let value = f64::from_bits(self.reservoir.values[self.idx].load(Relaxed));
            self.idx += 1;
            Some(value)
        } else {
            None
        }
    }
}

impl ExactSizeIterator for Drain<'_> {
    fn len(&self) -> usize {
        self.len - self.idx
    }
}

impl<'a> Drop for Drain<'a> {
    fn drop(&mut self) {
        self.reservoir.count.store(0, Release);
        self.reservoir.unsampled_sum.store(0.0f64.to_bits(), Release);
    }
}

/// An atomic sampling reservoir.
///
/// [Reservoir sampling][rs] is a technique used to produce a statistically representative sample of a data stream, in a
/// fixed space, without knowing the length of the stream in advance. `AtomicSamplingReservoir` is a thread-safe version of a
/// sampling reservoir, based on Vitter's ["Algorithm R"][vitter_paper].
///
/// Utilizes an A/B-based storage mechanism to avoid contention between producers and the consumer, and a fast,
/// thread-local PRNG ([Xoshiro256**][xoshiro256starstar]) to limit the per-call sampling overhead.
///
/// [rs]: https://en.wikipedia.org/wiki/Reservoir_sampling
/// [vitter_paper]: https://www.cs.umd.edu/~samir/498/vitter.pdf
/// [xoshiro256starstar]: https://prng.di.unimi.it
pub struct AtomicSamplingReservoir {
    primary: Reservoir,
    secondary: Reservoir,
    use_primary: AtomicBool,
    swap: Mutex<()>,
}

impl AtomicSamplingReservoir {
    /// Creates a new `AtomicSamplingReservoir` that stores up to `size` samples.
    pub fn new(size: usize) -> Self {
        Self {
            primary: Reservoir::with_capacity(size),
            secondary: Reservoir::with_capacity(size),
            use_primary: AtomicBool::new(true),
            swap: Mutex::new(()),
        }
    }

    /// Returns `true` if the reservoir is empty.
    pub fn is_empty(&self) -> bool {
        let use_primary = self.use_primary.load(Acquire);
        if use_primary {
            self.primary.count.load(Relaxed) == 0
        } else {
            self.secondary.count.load(Relaxed) == 0
        }
    }

    /// Pushes a sample into the reservoir.
    pub fn push(&self, value: f64) {
        let use_primary = self.use_primary.load(Relaxed);
        if use_primary {
            self.primary.push(value);
        } else {
            self.secondary.push(value);
        };
    }

    /// Consumes all samples in the reservoir, passing them to the provided closure.
    ///
    /// The underlying storage is swapped before the closure is called, and the previous storage is consumed.
    pub fn consume<F>(&self, mut f: F)
    where
        F: FnMut(Drain<'_>),
    {
        let _guard = self.swap.lock().unwrap();

        // Swap the active reservoir atomically.
        let use_primary = self.use_primary.fetch_xor(true, AcqRel);

        // Consume the previous reservoir.
        let drain = if use_primary { self.primary.drain() } else { self.secondary.drain() };

        f(drain);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_under_capacity_no_overflow() {
        let reservoir = AtomicSamplingReservoir::new(64);

        for i in 0..10 {
            reservoir.push(i as f64);
        }

        reservoir.consume(|drain| {
            assert_eq!(drain.unsampled_len(), 10);
            assert_eq!(drain.unsampled_sum(), 45.0);
            assert_eq!(drain.sample_rate(), 1.0);

            let collected: Vec<f64> = drain.collect();
            assert_eq!(collected.len(), 10);
        });
    }

    #[test]
    fn test_overflow_unsampled_len_and_sum_are_true_values() {
        let reservoir = AtomicSamplingReservoir::new(16);

        for i in 0..1000 {
            reservoir.push(i as f64);
        }

        reservoir.consume(|drain| {
            assert_eq!(drain.unsampled_len(), 1000);
            assert_eq!(drain.unsampled_sum(), 499500.0);
            assert!(drain.sample_rate() < 1.0);

            let collected: Vec<f64> = drain.collect();
            assert_eq!(collected.len(), 16);

            let sampled_sum: f64 = collected.iter().sum();
            assert!(sampled_sum < 499500.0);
        });
    }

    #[test]
    fn test_reset_after_drain() {
        let reservoir = AtomicSamplingReservoir::new(64);

        for i in 0..100 {
            reservoir.push(i as f64);
        }

        reservoir.consume(|_drain| {
        });

        for i in 0..50 {
            reservoir.push(i as f64);
        }

        reservoir.consume(|drain| {
            assert_eq!(drain.unsampled_len(), 50);
            assert_eq!(drain.unsampled_sum(), 1225.0);
        });
    }

    #[test]
    fn test_empty_reservoir() {
        let reservoir = AtomicSamplingReservoir::new(64);

        reservoir.consume(|drain| {
            assert_eq!(drain.unsampled_len(), 0);
            assert_eq!(drain.unsampled_sum(), 0.0);
        });
    }
}
