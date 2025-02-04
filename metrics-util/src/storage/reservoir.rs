//! An atomic sampling reservoir.

use std::{
    cell::UnsafeCell,
    sync::{
        atomic::{
            AtomicBool, AtomicU64, AtomicUsize,
            Ordering::{Acquire, Relaxed, Release},
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
}

impl Reservoir {
    fn with_capacity(capacity: usize) -> Self {
        let mut values = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            values.push(AtomicU64::new(0));
        }

        Self { values: values.into_boxed_slice(), count: AtomicUsize::new(0) }
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
    }

    fn drain(&self) -> Drain<'_> {
        let unsampled_len = self.count.load(Relaxed);
        let len = if unsampled_len > self.values.len() { self.values.len() } else { unsampled_len };
        Drain { reservoir: self, unsampled_len, len, idx: 0 }
    }
}

/// A draining iterator over the samples in a reservoir.
pub struct Drain<'a> {
    reservoir: &'a Reservoir,
    unsampled_len: usize,
    len: usize,
    idx: usize,
}

impl<'a> Drain<'a> {
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

        // Swap the active reservoir.
        let use_primary = self.use_primary.load(Acquire);
        self.use_primary.store(!use_primary, Release);

        // Consume the previous reservoir.
        let drain = if use_primary { self.primary.drain() } else { self.secondary.drain() };

        f(drain);
    }
}
