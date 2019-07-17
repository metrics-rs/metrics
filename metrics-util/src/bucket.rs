use crossbeam_epoch::{pin as epoch_pin, Atomic, Guard, Owned, Shared};
use std::{
    cell::UnsafeCell,
    mem, slice,
    sync::atomic::{AtomicUsize, Ordering},
};

const BLOCK_SIZE: usize = 128;

/// Discrete chunk of values with atomic read/write access.
struct Block<T> {
    // Write index.
    write: AtomicUsize,

    // Read index.
    read: AtomicUsize,

    // The individual slots.
    slots: [UnsafeCell<T>; BLOCK_SIZE],

    // The next block before this one.
    prev: Atomic<Block<T>>,
}

impl<T> Block<T> {
    /// Creates a new [`Block`].
    pub fn new() -> Self {
        Block {
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
            slots: unsafe { mem::zeroed() },
            prev: Atomic::null(),
        }
    }

    /// Gets the current length of this block.
    pub fn len(&self) -> usize {
        self.read.load(Ordering::Acquire)
    }

    /// Gets a slice of the data written to this block.
    pub fn data(&self) -> &[T] {
        let len = self.len();
        let head = self.slots[0].get();
        unsafe { slice::from_raw_parts(head as *const T, len) }
    }

    /// Links this block to the previous block in the bucket.
    pub fn set_prev(&self, prev: Shared<Block<T>>, guard: &Guard) {
        match self
            .prev
            .compare_and_set(Shared::null(), prev, Ordering::AcqRel, guard)
        {
            Ok(_) => {}
            Err(_) => unreachable!(),
        }
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
        self.read.fetch_add(1, Ordering::AcqRel);

        Ok(())
    }
}

unsafe impl<T> Send for Block<T> {}
unsafe impl<T> Sync for Block<T> {}

impl<T> Drop for Block<T> {
    fn drop(&mut self) {
        let guard = &epoch_pin();
        let prev = self.prev.swap(Shared::null(), Ordering::AcqRel, guard);
        if !prev.is_null() {
            unsafe {
                guard.defer_destroy(prev);
            }
            guard.flush();
        }
    }
}

/// An atomic bucket with snapshot capabilities.
///
/// This bucket is implemented as a singly-linked list of blocks, where each block is a small
/// buffer that can hold a handful of elements.  There is no limit to how many elements can be in
/// the bucket at a time.  Blocks are dynamically allocated as elements are pushed into the bucket.
///
/// Unlike a queue, buckets cannot be drained element by element: callers must iterate the whole
/// structure.  Reading the bucket happens in reverse, to allow writers to make forward progress
/// without affecting the iteration of the previously-written values.
///
/// The bucket can be cleared while a concurrent snapshot is taking place, and will not affect the
/// reader.
#[derive(Debug)]
pub struct AtomicBucket<T> {
    tail: Atomic<Block<T>>,
}

impl<T> AtomicBucket<T> {
    /// Creates a new, empty bucket.
    pub fn new() -> Self {
        Self::default()
    }

    /// Pushes an element into the bucket.
    pub fn push(&self, value: T) {
        let mut original = value;
        loop {
            // Load the tail block, or install a new one.
            let guard = &epoch_pin();
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
                        // the previous block.
                        Ok(ptr) => {
                            let new_tail = unsafe { ptr.deref() };
                            new_tail.set_prev(tail, guard);

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
                        Err(_) => {
                            original = value;
                            continue;
                        }
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

        // While we have a valid block -- either `tail` or the next block as we keep reading -- we
        // load the data from each block and process it by calling `f`.
        let mut block_ptr = self.tail.load(Ordering::Acquire, guard);
        while !block_ptr.is_null() {
            let block = unsafe { block_ptr.deref() };

            // Read the data out of the block.
            let data = block.data();
            f(data);

            // Load the next block.
            block_ptr = block.prev.load(Ordering::Acquire, guard);
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
        // We simply swap the tail pointer which effectively clears the bucket.  Callers might
        // still be in process of writing to the tail node, or reading the data, but new callers
        // will see it as empty until another write proceeds.
        let guard = &epoch_pin();
        let tail = self.tail.load(Ordering::Acquire, guard);
        if !tail.is_null()
            && self
                .tail
                .compare_and_set(tail, Shared::null(), Ordering::SeqCst, guard)
                .is_ok()
        {
            // We won the swap to delete the tail node.  Now configure a deferred drop to clean
            // things up once nobody else is using it.
            unsafe {
                // Drop the block, which will cause a cascading drop on the next block, and
                // so on and so forth, until all blocks linked to this one are dropped.
                guard.defer_destroy(tail);
            }
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

        let res = crossbeam::scope(|s| {
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

        let res = crossbeam::scope(|s| {
            let t1 = s.spawn(|_| {
                let mut i = 0;
                let mut total = 0;
                while i < BLOCK_SIZE as u64 * 10_000 {
                    bucket.push(i);

                    total += i;
                    i += 1;
                }
                total
            });

            let t2 = s.spawn(|_| {
                let mut i = 0;
                let mut total = 0;
                while i < BLOCK_SIZE as u64 * 10_000 {
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
        assert_eq!(snapshot.len(), BLOCK_SIZE * 20_000);

        let sum: u64 = snapshot.iter().sum();
        assert_eq!(sum, total);
    }
}
