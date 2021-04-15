use crossbeam_epoch::{pin as epoch_pin, unprotected, Atomic, Guard, Owned, Shared};
use crossbeam_utils::Backoff;
use std::{
    cell::UnsafeCell,
    cmp::min,
    mem, slice,
    sync::atomic::{AtomicUsize, Ordering},
};

#[cfg(target_pointer_width = "16")]
const BLOCK_SIZE: usize = 16;
#[cfg(target_pointer_width = "32")]
const BLOCK_SIZE: usize = 32;
#[cfg(target_pointer_width = "64")]
const BLOCK_SIZE: usize = 64;

const DEFERRED_BLOCK_BATCH_SIZE: usize = 32;

/// Discrete chunk of values with atomic read/write access.
struct Block<T> {
    // Write index.
    write: AtomicUsize,

    // Read bitmap.
    read: AtomicUsize,

    // The individual slots.
    slots: [UnsafeCell<T>; BLOCK_SIZE],

    // The "next" block to iterate, aka the block that came before this one.
    next: Atomic<Block<T>>,
}

impl<T> Block<T> {
    /// Creates a new [`Block`].
    pub fn new() -> Self {
        Block {
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
            slots: unsafe { mem::zeroed() },
            next: Atomic::null(),
        }
    }

    // Gets the length of the next block, if it exists.
    pub(crate) fn next_len(&self, guard: &Guard) -> usize {
        let tail = self.next.load(Ordering::Acquire, guard);
        if tail.is_null() {
            return 0;
        }

        let tail_block = unsafe { tail.deref() };
        tail_block.len()
    }

    /// Gets the current length of this block.
    pub fn len(&self) -> usize {
        self.read.load(Ordering::Acquire).trailing_ones() as usize
    }

    // Whether or not this block is currently quieseced i.e. no in-flight writes.
    pub fn is_quiesced(&self) -> bool {
        let len = self.len();
        if len == BLOCK_SIZE {
            return true;
        }

        // We have to clamp self.write since multiple threads might race on filling the last block,
        // so the value could actually exceed BLOCK_SIZE.
        min(self.write.load(Ordering::Acquire), BLOCK_SIZE) == len
    }

    /// Gets a slice of the data written to this block.
    pub fn data(&self) -> &[T] {
        let len = self.len();
        let head = self.slots[0].get();
        unsafe { slice::from_raw_parts(head as *const T, len) }
    }

    /// Pushes a value into this block.
    pub fn push(&self, value: T) -> Result<(), T> {
        // Try to increment the index.  If we've reached the end of the block, let the bucket know
        // so it can attach another block.
        let index = self.write.fetch_add(1, Ordering::AcqRel);
        if index >= BLOCK_SIZE {
            return Err(value);
        }

        // Update the slot.
        unsafe {
            self.slots[index].get().write(value);
        }

        // Scoot our read index forward.
        self.read.fetch_or(1 << index, Ordering::AcqRel);

        Ok(())
    }
}

unsafe impl<T: Send> Send for Block<T> {}
unsafe impl<T: Sync> Sync for Block<T> {}

impl<T> std::fmt::Debug for Block<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let has_next = unsafe { !self.next.load(Ordering::Acquire, &unprotected()).is_null() };
        f.debug_struct("Block")
            .field("type", &std::any::type_name::<T>())
            .field("block_size", &BLOCK_SIZE)
            .field("write", &self.write.load(Ordering::Acquire))
            .field("read", &self.read.load(Ordering::Acquire))
            .field("len", &self.len())
            .field("has_next", &has_next)
            .finish()
    }
}

/// A lock-free bucket with snapshot capabilities.
///
/// This bucket is implemented as a singly-linked list of blocks, where each block is a small
/// buffer that can hold a handful of elements.  There is no limit to how many elements can be in
/// the bucket at a time.  Blocks are dynamically allocated as elements are pushed into the bucket.
///
/// Unlike a queue, buckets cannot be drained element by element: callers must iterate the whole
/// structure.  Reading the bucket happens in a quasi-reverse fashion, to allow writers to make
/// forward progress without affecting the iteration of the previously written values.
///
/// For example, in a scenario where an internal block can hold 4 elements, and the caller has
/// written 10 elements to the bucket, you would expect to see the values in this order when iterating:
///
/// [6 7 8 9] [2 3 4 5] [0 1]
///
/// Block sizes are dependent on the target architecture, where each block can hold N items, and N
/// is the number of bits in the target architecture's pointer width.
#[derive(Debug)]
pub struct AtomicBucket<T> {
    tail: Atomic<Block<T>>,
}

impl<T> AtomicBucket<T> {
    /// Creates a new, empty bucket.
    pub fn new() -> Self {
        AtomicBucket {
            tail: Atomic::null(),
        }
    }

    /// Checks whether or not this bucket is empty.
    pub fn is_empty(&self) -> bool {
        let guard = &epoch_pin();
        let tail = self.tail.load(Ordering::Acquire, guard);
        if tail.is_null() {
            return true;
        }

        // We have to check the next block of our tail in case the current tail is simply a fresh
        // block that has not been written to yet.
        let tail_block = unsafe { tail.deref() };
        tail_block.len() == 0 && tail_block.next_len(&guard) == 0
    }

    /// Pushes an element into the bucket.
    pub fn push(&self, value: T) {
        let mut original = value;
        let guard = &epoch_pin();
        loop {
            // Load the tail block, or install a new one.
            let mut tail = self.tail.load(Ordering::Acquire, guard);
            if tail.is_null() {
                // No blocks at all yet.  We need to create one.
                match self.tail.compare_and_set(
                    Shared::null(),
                    Owned::new(Block::new()),
                    Ordering::AcqRel,
                    guard,
                ) {
                    // We won the race to install the new block.
                    Ok(ptr) => tail = ptr,
                    // Somebody else beat us, so just update our pointer.
                    Err(e) => tail = e.current,
                }
            }

            // We have a block now, so we need to try writing to it.
            let tail_block = unsafe { tail.deref() };
            match tail_block.push(original) {
                // If the push was OK, then the block wasn't full.  It might _now_ be full, but we'll
                // let future callers deal with installing a new block if necessary.
                Ok(_) => return,
                // The block was full, so we've been given the value back and we need to install a new block.
                Err(value) => {
                    match self.tail.compare_and_set(
                        tail,
                        Owned::new(Block::new()),
                        Ordering::AcqRel,
                        guard,
                    ) {
                        // We managed to install the block, so we need to link this new block to
                        // the nextious block.
                        Ok(ptr) => {
                            let new_tail = unsafe { ptr.deref() };
                            new_tail.next.store(tail, Ordering::Release);

                            // Now push into our new block.
                            match new_tail.push(value) {
                                // We wrote the value successfully, so we're good here!
                                Ok(_) => return,
                                // The block was full, so just loop and start over.
                                Err(value) => {
                                    original = value;
                                    continue;
                                }
                            }
                        }
                        // Somebody else installed the block before us, so let's just start over.
                        Err(_) => original = value,
                    }
                }
            }
        }
    }

    /// Collects all of the elements written to the bucket.
    ///
    /// This operation can be slow as it involves allocating enough space to hold all of the
    /// elements within the bucket.  Consider [`data_with`](AtomicBucket::data_with) to incrementally iterate
    /// the internal blocks within the bucket.
    ///
    /// Elements are in partial reverse order: blocks are iterated in reverse order, but the
    /// elements within them will appear in their original order.
    pub fn data(&self) -> Vec<T>
    where
        T: Clone,
    {
        let mut values = Vec::new();
        self.data_with(|block| values.extend_from_slice(block));
        values
    }

    /// Iterates all of the elements written to the bucket, invoking `f` for each block.
    ///
    /// Elements are in partial reverse order: blocks are iterated in reverse order, but the
    /// elements within them will appear in their original order.
    pub fn data_with<F>(&self, mut f: F)
    where
        F: FnMut(&[T]),
    {
        let guard = &epoch_pin();
        let backoff = Backoff::new();

        // While we have a valid block -- either `tail` or the next block as we keep reading -- we
        // load the data from each block and process it by calling `f`.
        let mut block_ptr = self.tail.load(Ordering::Acquire, guard);
        while !block_ptr.is_null() {
            let block = unsafe { block_ptr.deref() };

            // We wait for the block to be quiesced to ensure we get any in-flight writes, and
            // snoozing specifically yields the reading thread to ensure things are given a
            // chance to complete.
            while !block.is_quiesced() {
                backoff.snooze();
            }

            // Read the data out of the block.
            let data = block.data();
            f(data);

            // Load the next block.
            block_ptr = block.next.load(Ordering::Acquire, guard);
        }
    }

    /// Clears the bucket.
    ///
    /// Deallocation of the internal blocks happens only when all readers have finished, and so
    /// will not necessarily occur during or immediately preceding this method.
    ///
    /// # Note
    /// This method will not affect reads that are already in progress.
    pub fn clear(&self) {
        self.clear_with(|_| {})
    }

    /// Clears the bucket, invoking `f` for every block that will be cleared.
    ///
    /// Deallocation of the internal blocks happens only when all readers have finished, and so
    /// will not necessarily occur during or immediately preceding this method.
    ///
    /// This method is useful for accumulating values and then observing them, in a way that allows
    /// the caller to avoid visiting the same values again the next time.
    ///
    /// This method allows a pattern of observing values before they're cleared, with a clear
    /// demarcation. A similar pattern used in the wild would be to have some data structure, like
    /// a vector, which is continuously filled, and then eventually swapped out with a new, empty
    /// vector, allowing the caller to read all of the old values while new values are being
    /// written, over and over again.
    ///
    /// # Note
    /// This method will not affect reads that are already in progress.
    pub fn clear_with<F>(&self, mut f: F)
    where
        F: FnMut(&[T]),
    {
        // We simply swap the tail pointer which effectively clears the bucket.  Callers might
        // still be in process of writing to the tail node, or reading the data, but new callers
        // will see it as empty until another write proceeds.
        let guard = &epoch_pin();
        let mut block_ptr = self.tail.load(Ordering::Acquire, guard);
        if !block_ptr.is_null()
            && self
                .tail
                .compare_and_set(block_ptr, Shared::null(), Ordering::SeqCst, guard)
                .is_ok()
        {
            let backoff = Backoff::new();
            let mut freeable_blocks = Vec::new();

            // While we have a valid block -- either `tail` or the next block as we keep reading -- we
            // load the data from each block and process it by calling `f`.
            while !block_ptr.is_null() {
                let block = unsafe { block_ptr.deref() };

                // We wait for the block to be quiesced to ensure we get any in-flight writes, and
                // snoozing specifically yields the reading thread to ensure things are given a
                // chance to complete.
                while !block.is_quiesced() {
                    backoff.snooze();
                }

                // Read the data out of the block.
                let data = block.data();
                f(data);

                // Load the next block and take the shared reference to the current.
                let old_block_ptr =
                    mem::replace(&mut block_ptr, block.next.load(Ordering::Acquire, guard));

                freeable_blocks.push(old_block_ptr);
                if freeable_blocks.len() >= DEFERRED_BLOCK_BATCH_SIZE {
                    let blocks = mem::replace(&mut freeable_blocks, Vec::new());
                    unsafe {
                        guard.defer_unchecked(move || {
                            for block in blocks {
                                drop(block.into_owned());
                            }
                        });
                    }
                }
            }

            // Free any remaining old blocks.
            if !freeable_blocks.is_empty() {
                unsafe {
                    guard.defer_unchecked(move || {
                        for block in freeable_blocks {
                            drop(block.into_owned());
                        }
                    });
                }
            }

            // This asks the global collector to attempt to drive execution of deferred operations a
            // little sooner than it may have done so otherwise.
            guard.flush();
        }
    }
}

impl<T> Default for AtomicBucket<T> {
    fn default() -> Self {
        Self {
            tail: Atomic::null(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AtomicBucket, Block, BLOCK_SIZE};
    use crossbeam_utils::thread::scope;

    #[test]
    fn test_create_new_block() {
        let block: Block<u64> = Block::new();
        assert_eq!(block.len(), 0);

        let data = block.data();
        assert_eq!(data.len(), 0);
    }

    #[test]
    fn test_block_write_then_read() {
        let block = Block::new();
        assert_eq!(block.len(), 0);

        let data = block.data();
        assert_eq!(data.len(), 0);

        let result = block.push(42);
        assert!(result.is_ok());
        assert_eq!(block.len(), 1);

        let data = block.data();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0], 42);
    }

    #[test]
    fn test_block_write_until_full_then_read() {
        let block = Block::new();
        assert_eq!(block.len(), 0);

        let data = block.data();
        assert_eq!(data.len(), 0);

        let mut i = 0;
        let mut total = 0;
        while i < BLOCK_SIZE as u64 {
            assert!(block.push(i).is_ok());

            total += i;
            i += 1;
        }

        let data = block.data();
        assert_eq!(data.len(), BLOCK_SIZE);

        let sum: u64 = data.iter().sum();
        assert_eq!(sum, total);

        let result = block.push(42);
        assert!(result.is_err());
    }

    #[test]
    fn test_block_write_until_full_then_read_mt() {
        let block = Block::new();
        assert_eq!(block.len(), 0);

        let data = block.data();
        assert_eq!(data.len(), 0);

        let res = scope(|s| {
            let t1 = s.spawn(|_| {
                let mut i = 0;
                let mut total = 0;
                while i < BLOCK_SIZE as u64 / 2 {
                    assert!(block.push(i).is_ok());

                    total += i;
                    i += 1;
                }
                total
            });

            let t2 = s.spawn(|_| {
                let mut i = 0;
                let mut total = 0;
                while i < BLOCK_SIZE as u64 / 2 {
                    assert!(block.push(i).is_ok());

                    total += i;
                    i += 1;
                }
                total
            });

            let t1_total = t1.join().unwrap();
            let t2_total = t2.join().unwrap();

            t1_total + t2_total
        });

        let total = res.unwrap();

        let data = block.data();
        assert_eq!(data.len(), BLOCK_SIZE);

        let sum: u64 = data.iter().sum();
        assert_eq!(sum, total);

        let result = block.push(42);
        assert!(result.is_err());
    }

    #[test]
    fn test_bucket_write_then_read() {
        let bucket = AtomicBucket::new();
        bucket.push(42);

        let snapshot = bucket.data();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0], 42);
    }

    #[test]
    fn test_bucket_multiple_blocks_write_then_read() {
        let bucket = AtomicBucket::new();

        let snapshot = bucket.data();
        assert_eq!(snapshot.len(), 0);

        let target = (BLOCK_SIZE * 3 + BLOCK_SIZE / 2) as u64;
        let mut i = 0;
        let mut total = 0;
        while i < target {
            bucket.push(i);

            total += i;
            i += 1;
        }

        let snapshot = bucket.data();
        assert_eq!(snapshot.len(), target as usize);

        let sum: u64 = snapshot.iter().sum();
        assert_eq!(sum, total);
    }

    #[test]
    fn test_bucket_write_then_read_mt() {
        let bucket = AtomicBucket::new();

        let snapshot = bucket.data();
        assert_eq!(snapshot.len(), 0);

        let res = scope(|s| {
            let t1 = s.spawn(|_| {
                let mut i = 0;
                let mut total = 0;
                while i < BLOCK_SIZE as u64 * 100_000 {
                    bucket.push(i);

                    total += i;
                    i += 1;
                }
                total
            });

            let t2 = s.spawn(|_| {
                let mut i = 0;
                let mut total = 0;
                while i < BLOCK_SIZE as u64 * 100_000 {
                    bucket.push(i);

                    total += i;
                    i += 1;
                }
                total
            });

            let t1_total = t1.join().unwrap();
            let t2_total = t2.join().unwrap();

            t1_total + t2_total
        });

        let total = res.unwrap();

        let snapshot = bucket.data();
        assert_eq!(snapshot.len(), BLOCK_SIZE * 200_000);

        let sum = snapshot.iter().sum::<u64>();
        assert_eq!(sum, total);
    }

    #[test]
    fn test_clear_and_clear_with() {
        let bucket = AtomicBucket::new();

        let snapshot = bucket.data();
        assert_eq!(snapshot.len(), 0);

        let mut i = 0;
        let mut total_pushed = 0;
        while i < BLOCK_SIZE * 4 {
            bucket.push(i);

            total_pushed += i;
            i += 1;
        }

        let snapshot = bucket.data();
        assert_eq!(snapshot.len(), i);

        let mut total_accumulated = 0;
        bucket.clear_with(|xs| total_accumulated += xs.iter().sum::<usize>());
        assert_eq!(total_pushed, total_accumulated);

        let snapshot = bucket.data();
        assert_eq!(snapshot.len(), 0);
    }

    #[test]
    fn test_bucket_len_and_next_len() {
        let bucket = AtomicBucket::new();
        assert!(bucket.is_empty());

        let snapshot = bucket.data();
        assert_eq!(snapshot.len(), 0);

        // Just making sure that `is_empty` holds as we go from
        // the first block, to the second block, to exercise the
        // `Block::next_len` codepath.
        let mut i = 0;
        while i < BLOCK_SIZE * 2 {
            bucket.push(i);
            assert!(!bucket.is_empty());
            i += 1;
        }
    }
}
