use crossbeam_utils::CachePadded;
use std::{
    alloc::{alloc, dealloc, handle_alloc_error, Layout},
    cell::Cell,
    cmp::min,
    marker::PhantomData,
    mem, ptr, slice,
    sync::PoisonError,
};

#[cfg(loom)]
use loom::sync::{
    atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering},
    Mutex,
};
#[cfg(not(loom))]
use {
    crossbeam_utils::Backoff,
    std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering},
    std::sync::Mutex,
};

/// Number of slots in the first block of a stripe.
///
/// Blocks double in size as a stripe grows, up to `MAX_BLOCK_SIZE`, so that a stripe holding only
/// a few values costs only a small block, while a busy stripe quickly gets to full-size blocks.
///
/// Under loom, blocks start tiny so that the tests hit block boundaries within a few pushes.
const MIN_BLOCK_SIZE: usize = if cfg!(loom) { 2 } else { 8 };

/// Number of slots in a block, at most.
const MAX_BLOCK_SIZE: usize = 64;

/// Upper bound on the number of stripes in a bucket.
///
/// The actual number of stripes is derived from the available parallelism of the machine, rounded
/// up to a power of two, so this only kicks in on very large machines.
#[cfg(not(loom))]
const MAX_STRIPES: usize = 256;

/// Discrete chunk of values with atomic read/write access.
///
/// A block is a single allocation: this header, followed by `capacity` "ready" flags, followed by
/// `capacity` slots.  The flags and slots live outside of `Block<T>` itself, so a block is only
/// ever handled through a raw pointer to the allocation, and every method takes one.  See
/// [`allocate`](Block::allocate) and [`free`](Block::free).
struct Block<T> {
    // Write index.
    //
    // Internally, we track the write index which indicates what slot should be written by the next
    // writer.  This works fine as writers race via CAS to "acquire" a slot to write to.  The
    // trouble comes when attempting to read written values, as writers may still have writes
    // in-flight, thus leading to potential uninitialized reads, UB, and the world imploding.
    //
    // We use a simple scheme where writers acknowledge their writes by setting the "ready" flag
    // that corresponds to the index that they've written, with a release store, so that publishing
    // a value costs a plain store rather than a read-modify-write.  The number of initialized slots
    // is then the length of the leading run of set flags, which readers compute by scanning the
    // flags; that's a linear scan, but over at most a cache line, and readers are rare compared to
    // writers.
    write: AtomicUsize,

    // The "next" block to iterate, aka the block that came before this one.
    next: AtomicPtr<Block<T>>,

    // Number of slots, and of ready flags, that follow the header.
    capacity: usize,

    _values: PhantomData<T>,
}

impl<T> Block<T> {
    // The ready flags follow the header, at the first suitably aligned offset.
    const FLAGS_OFFSET: usize = align_up(mem::size_of::<Self>(), mem::align_of::<AtomicBool>());

    // The slots follow the flags, at the first suitably aligned offset.
    fn slots_offset(capacity: usize) -> usize {
        align_up(Self::FLAGS_OFFSET + capacity * mem::size_of::<AtomicBool>(), mem::align_of::<T>())
    }

    // The layout of the allocation for a block with the given capacity.
    fn layout(capacity: usize) -> Layout {
        let size = Self::slots_offset(capacity) + capacity * mem::size_of::<T>();
        let align =
            mem::align_of::<Self>().max(mem::align_of::<AtomicBool>()).max(mem::align_of::<T>());
        Layout::from_size_align(align_up(size, align), align).expect("block capacity overflow")
    }

    /// Allocates a new, empty block with room for `capacity` values.
    fn allocate(capacity: usize) -> *mut Self {
        let layout = Self::layout(capacity);

        // SAFETY: The layout is never zero-sized, as it includes the header.
        let block = unsafe { alloc(layout) }.cast::<Self>();
        if block.is_null() {
            handle_alloc_error(layout);
        }

        // SAFETY:
        // We initialize every part of the block other than the slots, which stay uninitialized
        // until their ready flag is set.  The atomics are constructed rather than zeroed, as that
        // is what loom's atomics require.
        unsafe {
            ptr::addr_of_mut!((*block).write).write(AtomicUsize::new(0));
            ptr::addr_of_mut!((*block).next).write(AtomicPtr::new(ptr::null_mut()));
            ptr::addr_of_mut!((*block).capacity).write(capacity);
            ptr::addr_of_mut!((*block)._values).write(PhantomData);

            let flags = block.cast::<u8>().add(Self::FLAGS_OFFSET).cast::<AtomicBool>();
            for index in 0..capacity {
                flags.add(index).write(AtomicBool::new(false));
            }
        }

        block
    }

    /// Frees a block, dropping the values in it.
    ///
    /// # Safety
    ///
    /// The caller must have exclusive ownership of the block, and every operation that may have
    /// observed it must have completed.
    unsafe fn free(block: *mut Self) {
        spin_until(|| Self::is_quiesced(block));

        // SAFETY (for the drops): a slot's ready flag is only set once the slot has been fully
        // written, guaranteeing that every slot below `len` is initialized.
        let len = Self::len(block);
        for index in 0..len {
            ptr::drop_in_place(Self::slot(block, index));
        }

        // Under loom, the block is poisoned and leaked instead of freed, so that a protocol bug
        // that lets a thread touch a freed block shows up as garbage values in the loom tests,
        // rather than as undefined behaviour that the model checker cannot observe.
        if cfg!(loom) {
            let capacity = (*block).capacity;
            ptr::write_bytes(
                Self::slot(block, 0).cast::<u8>(),
                0xff,
                capacity * mem::size_of::<T>(),
            );
            return;
        }

        dealloc(block.cast(), Self::layout((*block).capacity));
    }

    // Gets the ready flags of the block.
    unsafe fn flags<'a>(block: *const Self) -> &'a [AtomicBool] {
        let flags = block.cast::<u8>().add(Self::FLAGS_OFFSET).cast::<AtomicBool>();
        slice::from_raw_parts(flags, (*block).capacity)
    }

    // Gets a pointer to the slot at `index`.
    unsafe fn slot(block: *const Self, index: usize) -> *mut T {
        let slots = block.cast::<u8>().add(Self::slots_offset((*block).capacity)).cast::<T>();
        slots.add(index).cast_mut()
    }

    // Gets the length of the next block, if it exists.
    //
    // SAFETY: The caller must ensure that neither block can be freed while this call is in
    // progress, which is to say that the caller must have entered the owning stripe.
    unsafe fn next_len(block: *const Self) -> usize {
        let next = (*block).next.load(Ordering::Acquire);
        if next.is_null() {
            return 0;
        }

        Self::len(next)
    }

    /// Gets the current length of this block.
    unsafe fn len(block: *const Self) -> usize {
        Self::flags(block).iter().take_while(|ready| ready.load(Ordering::Acquire)).count()
    }

    // Whether or not this block is currently quieseced i.e. no in-flight writes.
    unsafe fn is_quiesced(block: *const Self) -> bool {
        let capacity = (*block).capacity;
        let len = Self::len(block);
        if len == capacity {
            return true;
        }

        // We have to clamp `write` since multiple threads might race on filling the last slot, so
        // the value could actually exceed the capacity.
        min((*block).write.load(Ordering::Acquire), capacity) == len
    }

    /// Gets a slice of the data written to this block.
    unsafe fn data<'a>(block: *const Self) -> &'a [T] {
        // SAFETY:
        // The slice is only as long as the number of slots written, indicated by `len`.  A slot's
        // ready flag is only set once the slot has been fully written, guaranteeing that every
        // slot in the slice is initialized, and slots are never written again once initialized.
        slice::from_raw_parts(Self::slot(block, 0), Self::len(block))
    }

    /// Pushes a value into this block.
    unsafe fn push(block: *const Self, value: T) -> Result<(), T> {
        // Try to increment the index.  If we've reached the end of the block, let the stripe know
        // so it can attach another block.
        let index = (*block).write.fetch_add(1, Ordering::Relaxed);
        if index >= (*block).capacity {
            return Err(value);
        }

        // SAFETY:
        // - We never index outside of the block's capacity.
        // - We own the slot, as the write index is only ever handed out once.
        // - We're writing an initialized value into the slot before anyone is able to ever read
        //   it, ensuring no uninitialized access.
        Self::slot(block, index).write(value);

        // Publish the slot.
        Self::flags(block)[index].store(true, Ordering::Release);

        Ok(())
    }
}

/// Rounds `value` up to the next multiple of `align`, which must be a power of two.
const fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

/// Frees a singly-linked list of blocks, starting at `block`.
///
/// # Safety
///
/// The caller must have exclusive ownership of every block in the list: the list must have been
/// detached from its stripe, and every operation that may have observed the blocks must have
/// completed.
unsafe fn free_blocks<T>(mut block: *mut Block<T>) {
    while !block.is_null() {
        let next = (*block).next.load(Ordering::Acquire);
        Block::free(block);
        block = next;
    }
}

/// A single writer-affine stripe of a bucket.
///
/// Each stripe is its own singly-linked list of blocks, and is only ever pushed to by the threads
/// that map to it (usually a single thread), so pushes do not contend on shared cache lines.
///
/// Stripes also track in-flight operations, which is what allows a stripe to be cleared while
/// writers and readers are still using it: an operation "enters" the stripe before it touches the
/// list, and "exits" once it is done.  A clearer detaches the list and then waits until every
/// operation that entered before the detach has exited, at which point the detached blocks can no
/// longer be referenced by anyone and can be freed.
///
/// In-flight operations are counted in one of two phases.  Operations enter in the current
/// phase, and clearing flips the phase before waiting for the previous phase's count to drain,
/// so that the wait cannot be starved by operations that keep entering: those all land in the new
/// phase.  The phase and both counts live in a single atomic word, so that entering and flipping
/// are ordered by that word's modification order: an operation either enters before the flip, in
/// which case the clearer waits for it, or after it, in which case it observes the detached list
/// as already gone.
struct Stripe<T> {
    // The most recently attached block.
    tail: AtomicPtr<Block<T>>,

    // The current phase and the in-flight count for each phase.  See `PHASE_BIT` and friends.
    state: AtomicUsize,

    // Serializes clearers, so that only one of them flips the phase at a time.
    clearing: Mutex<()>,
}

// The top bit of `Stripe::state` is the current phase, and the remaining bits are split evenly
// into the in-flight count for phase 0 (low bits) and phase 1 (high bits).
const PHASE_BIT: usize = 1 << (usize::BITS - 1);
const COUNT_BITS: u32 = (usize::BITS - 1) / 2;
const COUNT_MASK: usize = (1 << COUNT_BITS) - 1;

#[inline]
fn phase_of(state: usize) -> usize {
    (state & PHASE_BIT != 0) as usize
}

#[inline]
fn count_unit(phase: usize) -> usize {
    1 << (phase as u32 * COUNT_BITS)
}

#[inline]
fn count_of(state: usize, phase: usize) -> usize {
    (state >> (phase as u32 * COUNT_BITS)) & COUNT_MASK
}

impl<T> Stripe<T> {
    fn new() -> Self {
        Stripe {
            tail: AtomicPtr::new(ptr::null_mut()),
            state: AtomicUsize::new(0),
            clearing: Mutex::new(()),
        }
    }

    /// Marks an operation as having entered the stripe.
    ///
    /// Any block reachable from `tail` after this call cannot be freed until the returned guard
    /// is dropped, which marks the operation as having exited.
    fn enter(&self) -> InFlight<'_, T> {
        // We have to increment the count of whatever the current phase is at the moment of the
        // increment, but the phase can only be read together with the counts, so we guess it
        // from a plain load, increment, and check the phase we actually observed.  If we guessed
        // wrong, we undo the increment and try again; that's rare, as it takes a clear between
        // the load and the increment.  Once we've incremented the count of the current phase: if
        // we're ordered after a phase flip, then we acquire the clearer's detach of the list, and
        // will observe the tail as null or newer; if we're ordered before it, then the clearer
        // will see our entry in the previous phase and wait for us to exit.
        loop {
            let phase = phase_of(self.state.load(Ordering::Relaxed));
            let previous = self.state.fetch_add(count_unit(phase), Ordering::Acquire);
            if phase_of(previous) == phase {
                return InFlight { stripe: self, phase };
            }

            self.state.fetch_sub(count_unit(phase), Ordering::Release);
        }
    }

    /// Marks an operation that entered in the given phase as having exited the stripe.
    fn exit(&self, phase: usize) {
        // Release, so that a clearer that observes our exit also observes everything we did to
        // the blocks before it.
        self.state.fetch_sub(count_unit(phase), Ordering::Release);
    }

    /// Detaches the current list of blocks from the stripe, returning the first block.
    ///
    /// Once this method returns, no other operation can reach the detached blocks, so they may be
    /// freed by the caller after they've been processed.
    ///
    /// The caller must hold the `clearing` lock.
    fn detach(&self) -> *mut Block<T> {
        let detached = self.tail.swap(ptr::null_mut(), Ordering::AcqRel);
        if detached.is_null() {
            return detached;
        }

        // Flip the phase, and then wait for every operation that entered in the previous phase
        // to exit.  Any operation that could have observed the detached blocks must have entered
        // in the previous phase, and every operation that enters from here on lands in the new
        // phase, so the previous phase's count can only go down.
        let previous = phase_of(self.state.fetch_xor(PHASE_BIT, Ordering::AcqRel));
        spin_until(|| count_of(self.state.load(Ordering::Acquire), previous) == 0);

        detached
    }

    /// Pushes a value into this stripe.
    fn push(&self, value: T) {
        let _in_flight = self.enter();

        let mut original = value;
        loop {
            // Load the tail block, or install a new one.
            let mut tail = self.tail.load(Ordering::Acquire);
            if tail.is_null() {
                // No blocks at all yet.  We need to create one.
                let new_block = Block::allocate(MIN_BLOCK_SIZE);
                match self.tail.compare_exchange(
                    ptr::null_mut(),
                    new_block,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    // We won the race to install the new block.
                    Ok(_) => tail = new_block,
                    // Somebody else beat us, so just update our pointer.
                    Err(current) => {
                        // SAFETY: We just allocated this block and nobody else has seen it.
                        unsafe { Block::free(new_block) };
                        tail = current;
                    }
                }
            }

            // We have a block now, so we need to try writing to it.
            //
            // SAFETY: We've entered the stripe, so the tail block cannot be freed until we exit.
            match unsafe { Block::push(tail, original) } {
                // If the push was OK, then the block wasn't full.  It might _now_ be full, but we'll
                // let future callers deal with installing a new block if necessary.
                Ok(()) => break,
                // The block was full, so we've been given the value back and we need to install a new block.
                Err(value) => {
                    // Blocks double in size until they hit the cap, so that a stripe that only
                    // ever holds a handful of values doesn't pay for a full-size block.
                    //
                    // SAFETY: We've entered the stripe, so the tail block cannot be freed, and
                    // nobody else can see the new block until we install it below.
                    let capacity = min(unsafe { (*tail).capacity } * 2, MAX_BLOCK_SIZE);
                    let new_block = Block::allocate(capacity);
                    unsafe { (*new_block).next.store(tail, Ordering::Release) };

                    match self.tail.compare_exchange(
                        tail,
                        new_block,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    ) {
                        // We managed to install the block, so now push into it.
                        Ok(_) => {
                            // SAFETY: We've entered the stripe, so the block cannot be freed.
                            match unsafe { Block::push(new_block, value) } {
                                // We wrote the value successfully, so we're good here!
                                Ok(()) => break,
                                // The block was full, so just loop and start over.
                                Err(value) => original = value,
                            }
                        }
                        // Somebody else installed a block (or the stripe was cleared) before us,
                        // so let's just start over.
                        Err(_) => {
                            // SAFETY: We just allocated this block and nobody else has seen it.
                            unsafe { Block::free(new_block) };
                            original = value;
                        }
                    }
                }
            }
        }
    }

    /// Whether or not this stripe has any values in it.
    fn is_empty(&self) -> bool {
        let _in_flight = self.enter();

        let tail = self.tail.load(Ordering::Acquire);
        if tail.is_null() {
            return true;
        }

        // We have to check the next block of our tail in case the current tail is simply a
        // fresh block that has not been written to yet.
        //
        // SAFETY: We've entered the stripe, so no block reachable from the tail can be freed.
        unsafe { Block::len(tail) == 0 && Block::next_len(tail) == 0 }
    }

    /// Iterates all of the values in this stripe, invoking `f` for each block.
    fn data_with<F>(&self, f: &mut F)
    where
        F: FnMut(&[T]),
    {
        let _in_flight = self.enter();

        // SAFETY: We've entered the stripe, so no block reachable from the tail can be freed, and
        // the guard exits the stripe even if `f` unwinds.
        unsafe { visit_blocks(self.tail.load(Ordering::Acquire), f) };
    }

    /// Clears this stripe, invoking `f` for each block before it's freed.
    fn clear_with<F>(&self, f: &mut F)
    where
        F: FnMut(&[T]),
    {
        let _guard = self.clearing.lock().unwrap_or_else(PoisonError::into_inner);

        // SAFETY: We've detached the blocks and waited for every operation that could have
        // observed them to exit, so we now exclusively own them, and they're freed when
        // `detached` is dropped, even if `f` unwinds.
        let detached = Detached(self.detach());
        unsafe { visit_blocks(detached.0, f) };
    }
}

/// An in-flight operation on a stripe, which exits the stripe when dropped.
struct InFlight<'a, T> {
    stripe: &'a Stripe<T>,
    phase: usize,
}

impl<T> Drop for InFlight<'_, T> {
    fn drop(&mut self) {
        self.stripe.exit(self.phase);
    }
}

/// A list of blocks that has been detached from its stripe, and is freed when dropped.
struct Detached<T>(*mut Block<T>);

impl<T> Drop for Detached<T> {
    fn drop(&mut self) {
        // SAFETY: A detached list is exclusively owned by whoever detached it.
        unsafe { free_blocks(self.0) }
    }
}

/// Visits every block in the list starting at `block`, invoking `f` with each block's data.
///
/// # Safety
///
/// The caller must ensure that no block in the list can be freed for the duration of the call.
unsafe fn visit_blocks<T, F>(mut block: *mut Block<T>, f: &mut F)
where
    F: FnMut(&[T]),
{
    // While we have a valid block -- either `tail` or the next block as we keep reading -- we
    // load the data from each block and process it by calling `f`.
    while !block.is_null() {
        // We wait for the block to be quiesced to ensure we get any in-flight writes.
        spin_until(|| Block::is_quiesced(block));

        // Read the data out of the block.
        f(Block::data(block));

        // Load the next block.
        block = (*block).next.load(Ordering::Acquire);
    }
}

/// Spins until `ready` returns true.
///
/// Snoozing yields the waiting thread, to ensure that whatever it's waiting on is given a chance to
/// complete.  Under loom, yielding is also what lets the model checker schedule the other threads.
fn spin_until(mut ready: impl FnMut() -> bool) {
    #[cfg(not(loom))]
    {
        let backoff = Backoff::new();
        while !ready() {
            backoff.snooze();
        }
    }

    #[cfg(loom)]
    while !ready() {
        loom::thread::yield_now();
    }
}

/// Gets the number of stripes used by every bucket.
///
/// This is the available parallelism of the machine, rounded up to a power of two, so that the
/// stripe index can be computed with a mask.
#[cfg(not(loom))]
fn stripe_count() -> usize {
    static COUNT: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *COUNT.get_or_init(|| {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .next_power_of_two()
            .clamp(1, MAX_STRIPES)
    })
}

/// Gets the stripe index for the current thread, given the number of stripes in a bucket.
///
/// Every thread is assigned a unique, sequential ID the first time it pushes into any bucket, so
/// the first N threads to push always land on distinct stripes of an N-stripe bucket.
#[cfg(not(loom))]
#[inline]
fn current_stripe_index(count: usize) -> usize {
    static NEXT_THREAD_ID: AtomicUsize = AtomicUsize::new(0);

    thread_local! {
        static THREAD_ID: Cell<usize> = const { Cell::new(usize::MAX) };
    }

    let id = THREAD_ID.with(|cell| {
        let mut id = cell.get();
        if id == usize::MAX {
            id = NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed);
            cell.set(id);
        }
        id
    });

    id & (count - 1)
}

/// Under loom, buckets have two stripes, so that the tests cover both writers that share a stripe
/// and writers on separate stripes.
#[cfg(loom)]
fn stripe_count() -> usize {
    2
}

#[cfg(loom)]
fn current_stripe_index(count: usize) -> usize {
    loom::lazy_static! {
        static ref NEXT_THREAD_ID: AtomicUsize = AtomicUsize::new(0);
    }

    loom::thread_local! {
        static THREAD_ID: Cell<usize> = Cell::new(usize::MAX);
    }

    let id = THREAD_ID.with(|cell| {
        let mut id = cell.get();
        if id == usize::MAX {
            id = NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed);
            cell.set(id);
        }
        id
    });

    id & (count - 1)
}

/// A lock-free bucket with snapshot capabilities.
///
/// This bucket is implemented as a set of per-thread stripes, where each stripe is a singly-linked
/// list of blocks, and each block is a small buffer that can hold a handful of elements.  There is
/// no limit to how many elements can be in the bucket at a time.  Stripes and blocks are
/// dynamically allocated as elements are pushed into the bucket, and blocks double in size as a
/// stripe grows, from 8 elements up to 64.
///
/// Each writing thread is mapped to a stripe, and only ever pushes into that stripe.  There are as
/// many stripes as the machine has available parallelism (rounded up to a power of two), so when
/// there are at most that many writers, every writer has a stripe to itself and pushes never
/// contend with each other.  Stripes and blocks are allocated the first time they're needed, so a
/// bucket that's only written from a few threads only pays for a few stripes.
///
/// Unlike a queue, buckets cannot be drained element by element: callers must iterate the whole
/// structure.  Reading the bucket happens stripe by stripe, and within each stripe, in a
/// quasi-reverse fashion, to allow writers to make forward progress without affecting the
/// iteration of the previously written values.
///
/// For example, if a single thread has written 30 elements to the bucket, you would expect to see
/// the values in this order when iterating:
///
/// ```text
/// [24 .. 29] [8 .. 23] [0 .. 7]
/// ```
///
/// When multiple threads have written to the bucket, the elements of each thread are grouped
/// together in the same fashion, but the order in which the groups appear is arbitrary.
pub struct AtomicBucket<T> {
    stripes: Box<[AtomicPtr<CachePadded<Stripe<T>>>]>,
}

// SAFETY: A bucket owns the values pushed into it, and hands out shared references to them from
// any thread, so it's only `Sync` if the values are `Send + Sync`, and it's `Send` if the values
// are `Send`.  The stripes and blocks themselves are safe to use from any thread.
unsafe impl<T: Send> Send for AtomicBucket<T> {}
unsafe impl<T: Send + Sync> Sync for AtomicBucket<T> {}

impl<T> AtomicBucket<T> {
    /// Creates a new, empty bucket.
    pub fn new() -> Self {
        let stripes =
            (0..stripe_count()).map(|_| AtomicPtr::new(ptr::null_mut())).collect::<Vec<_>>();
        AtomicBucket { stripes: stripes.into_boxed_slice() }
    }

    /// Checks whether or not this bucket is empty.
    pub fn is_empty(&self) -> bool {
        self.stripes().all(Stripe::is_empty)
    }

    /// Gets the stripe for the current thread, allocating it if necessary.
    #[inline]
    fn current_stripe(&self) -> &Stripe<T> {
        let index = current_stripe_index(self.stripes.len());

        // SAFETY: The index is masked to the number of stripes, which is a power of two.
        let slot = unsafe { self.stripes.get_unchecked(index) };

        let mut stripe = slot.load(Ordering::Acquire);
        if stripe.is_null() {
            let new_stripe = Box::into_raw(Box::new(CachePadded::new(Stripe::new())));
            match slot.compare_exchange(
                ptr::null_mut(),
                new_stripe,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                // We won the race to install the stripe.
                Ok(_) => stripe = new_stripe,
                // Somebody else beat us, so just update our pointer.
                Err(current) => {
                    // SAFETY: We just allocated this stripe and nobody else has seen it.
                    drop(unsafe { Box::from_raw(new_stripe) });
                    stripe = current;
                }
            }
        }

        // SAFETY: Stripes are only ever freed when the bucket is dropped, and we hold a reference
        // to the bucket.
        unsafe { &*stripe }
    }

    /// Iterates over every stripe that has been allocated so far.
    fn stripes(&self) -> impl Iterator<Item = &Stripe<T>> {
        self.stripes.iter().filter_map(|slot| {
            let stripe = slot.load(Ordering::Acquire);
            if stripe.is_null() {
                None
            } else {
                // SAFETY: Stripes are only ever freed when the bucket is dropped, and we hold a
                // reference to the bucket.
                Some(unsafe { &**stripe })
            }
        })
    }

    /// Pushes an element into the bucket.
    pub fn push(&self, value: T) {
        self.current_stripe().push(value);
    }

    /// Collects all of the elements written to the bucket.
    ///
    /// This operation can be slow as it involves allocating enough space to hold all of the
    /// elements within the bucket.  Consider [`data_with`](AtomicBucket::data_with) to incrementally iterate
    /// the internal blocks within the bucket.
    ///
    /// Elements are grouped by the thread that wrote them, in partial reverse order: blocks are
    /// iterated in reverse order, but the elements within them will appear in their original
    /// order.  The order of the groups is arbitrary.
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
    /// Elements are grouped by the thread that wrote them, in partial reverse order: blocks are
    /// iterated in reverse order, but the elements within them will appear in their original
    /// order.  The order of the groups is arbitrary.
    ///
    /// # Note
    /// `f` must not call [`clear`](AtomicBucket::clear) or [`clear_with`](AtomicBucket::clear_with)
    /// on this bucket, as clearing waits for in-progress reads to complete.
    pub fn data_with<F>(&self, mut f: F)
    where
        F: FnMut(&[T]),
    {
        for stripe in self.stripes() {
            stripe.data_with(&mut f);
        }
    }

    /// Clears the bucket.
    ///
    /// # Note
    /// This method waits for reads and writes that are already in progress to complete, and does
    /// not affect any values that are written after it has begun.
    pub fn clear(&self) {
        self.clear_with(|_: &[T]| {})
    }

    /// Clears the bucket, invoking `f` for every block that will be cleared.
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
    /// Elements are grouped by the thread that wrote them, in partial reverse order: blocks are
    /// iterated in reverse order, but the elements within them will appear in their original
    /// order.  The order of the groups is arbitrary.
    ///
    /// # Note
    /// This method waits for reads and writes that are already in progress to complete, and does
    /// not affect any values that are written after it has begun.  The internal blocks are freed
    /// as soon as `f` has been called for them.  Concurrent calls to this method are serialized,
    /// and `f` must not call [`clear`](AtomicBucket::clear) or `clear_with` on this bucket.
    pub fn clear_with<F>(&self, mut f: F)
    where
        F: FnMut(&[T]),
    {
        for stripe in self.stripes() {
            stripe.clear_with(&mut f);
        }
    }
}

impl<T> Default for AtomicBucket<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for AtomicBucket<T> {
    fn drop(&mut self) {
        // We have exclusive access to the bucket, so there can be no in-flight operations, and
        // every stripe and block still attached to the bucket is exclusively ours to free.
        for slot in self.stripes.iter() {
            let stripe = slot.load(Ordering::Acquire);
            if stripe.is_null() {
                continue;
            }

            // SAFETY: See above.
            unsafe {
                let stripe = Box::from_raw(stripe);
                free_blocks(stripe.tail.load(Ordering::Acquire));
                drop(stripe);
            }
        }
    }
}

impl<T> std::fmt::Debug for AtomicBucket<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let allocated = self.stripes().count();
        f.debug_struct("AtomicBucket")
            .field("type", &std::any::type_name::<T>())
            .field("stripes", &self.stripes.len())
            .field("allocated_stripes", &allocated)
            .finish()
    }
}

#[cfg(all(test, loom))]
mod loom_tests {
    use super::AtomicBucket;
    use loom::sync::Arc;
    use loom::thread;

    fn model<F>(f: F)
    where
        F: Fn() + Sync + Send + 'static,
    {
        // Exhaustive exploration of these scenarios takes far too long, so the number of
        // preemptions is bounded unless the environment says otherwise.  Every protocol bug
        // tried against these scenarios is caught with a bound of 2; the cost roughly triples
        // with each extra preemption, and 5 keeps the run to seconds.
        let mut builder = loom::model::Builder::new();
        if builder.preemption_bound.is_none() {
            builder.preemption_bound = Some(5);
        }
        builder.check(f);
    }

    /// Two writers push across block boundaries while the bucket is cleared underneath them, and
    /// every value must be seen exactly once between the concurrent clear and a final one.
    #[test]
    fn concurrent_pushes_and_clear() {
        model(|| {
            let bucket = Arc::new(AtomicBucket::new());

            let writers = (0..2)
                .map(|writer| {
                    let bucket = Arc::clone(&bucket);
                    thread::spawn(move || {
                        for i in 0..3usize {
                            bucket.push(writer * 10 + i);
                        }
                    })
                })
                .collect::<Vec<_>>();

            let mut seen = Vec::new();
            bucket.clear_with(|xs| seen.extend_from_slice(xs));

            for writer in writers {
                writer.join().unwrap();
            }
            bucket.clear_with(|xs| seen.extend_from_slice(xs));

            seen.sort_unstable();
            assert_eq!(seen, vec![0, 1, 2, 10, 11, 12]);
            assert!(bucket.is_empty());
        });
    }

    /// A reader walks the bucket while a writer pushes and a clearer frees blocks: the reader must
    /// only ever see values that were actually written (freed blocks are poisoned under loom, so
    /// reading one shows up as a bogus value), and the clears must see each value once.
    #[test]
    fn concurrent_push_read_and_clear() {
        model(|| {
            let bucket = Arc::new(AtomicBucket::new());

            let writer = {
                let bucket = Arc::clone(&bucket);
                thread::spawn(move || {
                    for i in 0..3usize {
                        bucket.push(i);
                    }
                })
            };

            let reader = {
                let bucket = Arc::clone(&bucket);
                thread::spawn(move || {
                    bucket.data_with(|xs| {
                        for x in xs {
                            assert!(*x < 3, "read a value that was never written: {}", x);
                        }
                    });
                })
            };

            let mut seen = Vec::new();
            bucket.clear_with(|xs| seen.extend_from_slice(xs));

            writer.join().unwrap();
            reader.join().unwrap();
            bucket.clear_with(|xs| seen.extend_from_slice(xs));

            seen.sort_unstable();
            assert_eq!(seen, vec![0, 1, 2]);
        });
    }

    /// Two clearers race each other while a writer pushes: between them they must see every value
    /// exactly once.
    #[test]
    fn concurrent_clears() {
        model(|| {
            let bucket = Arc::new(AtomicBucket::new());

            let writer = {
                let bucket = Arc::clone(&bucket);
                thread::spawn(move || {
                    for i in 0..3usize {
                        bucket.push(i);
                    }
                })
            };

            let clearer = {
                let bucket = Arc::clone(&bucket);
                thread::spawn(move || {
                    let mut seen = Vec::new();
                    bucket.clear_with(|xs| seen.extend_from_slice(xs));
                    seen
                })
            };

            let mut seen = Vec::new();
            bucket.clear_with(|xs| seen.extend_from_slice(xs));

            writer.join().unwrap();
            seen.extend(clearer.join().unwrap());
            bucket.clear_with(|xs| seen.extend_from_slice(xs));

            seen.sort_unstable();
            assert_eq!(seen, vec![0, 1, 2]);
        });
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::{AtomicBucket, Block, MAX_BLOCK_SIZE, MIN_BLOCK_SIZE};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Barrier;
    use std::thread;

    /// A block that's freed when dropped, for testing blocks on their own.
    struct OwnedBlock<T>(*mut Block<T>);

    unsafe impl<T: Send + Sync> Sync for OwnedBlock<T> {}

    impl<T> OwnedBlock<T> {
        fn new(capacity: usize) -> Self {
            OwnedBlock(Block::allocate(capacity))
        }

        fn push(&self, value: T) -> Result<(), T> {
            // SAFETY: We own the block, so it can't be freed while it's in use.
            unsafe { Block::push(self.0, value) }
        }

        fn len(&self) -> usize {
            // SAFETY: We own the block, so it can't be freed while it's in use.
            unsafe { Block::len(self.0) }
        }

        fn data(&self) -> &[T] {
            // SAFETY: We own the block, so it can't be freed while it's in use.
            unsafe { Block::data(self.0) }
        }
    }

    impl<T> Drop for OwnedBlock<T> {
        fn drop(&mut self) {
            // SAFETY: We own the block, and nothing else can be using it once we're dropped.
            unsafe { Block::free(self.0) }
        }
    }

    #[test]
    fn test_create_new_block() {
        let block: OwnedBlock<u64> = OwnedBlock::new(MAX_BLOCK_SIZE);
        assert_eq!(block.len(), 0);

        let data = block.data();
        assert_eq!(data.len(), 0);
    }

    #[test]
    fn test_block_write_then_read() {
        let block = OwnedBlock::new(MAX_BLOCK_SIZE);
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
        let block = OwnedBlock::new(MAX_BLOCK_SIZE);
        assert_eq!(block.len(), 0);

        let data = block.data();
        assert_eq!(data.len(), 0);

        let mut i = 0;
        let mut total = 0;
        while i < MAX_BLOCK_SIZE as u64 {
            assert!(block.push(i).is_ok());

            total += i;
            i += 1;
        }

        let data = block.data();
        assert_eq!(data.len(), MAX_BLOCK_SIZE);

        let sum: u64 = data.iter().sum();
        assert_eq!(sum, total);

        let result = block.push(42);
        assert!(result.is_err());
    }

    #[test]
    fn test_block_write_until_full_then_read_mt() {
        let block = OwnedBlock::new(MAX_BLOCK_SIZE);
        assert_eq!(block.len(), 0);

        let data = block.data();
        assert_eq!(data.len(), 0);

        let total = thread::scope(|s| {
            let writer = || {
                let mut total = 0;
                for i in 0..MAX_BLOCK_SIZE as u64 / 2 {
                    assert!(block.push(i).is_ok());
                    total += i;
                }
                total
            };
            let t1 = s.spawn(writer);
            let t2 = s.spawn(writer);
            t1.join().unwrap() + t2.join().unwrap()
        });

        let data = block.data();
        assert_eq!(data.len(), MAX_BLOCK_SIZE);

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

        let target = (MAX_BLOCK_SIZE * 3 + MAX_BLOCK_SIZE / 2) as u64;
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
    fn test_bucket_single_thread_order() {
        // Blocks are visited newest first, and values within a block are in insertion order.
        // Blocks double in size from `MIN_BLOCK_SIZE` up to `MAX_BLOCK_SIZE`, so the boundaries
        // fall at 8, 24, 56, 120, and then every 64 values.
        let bucket = AtomicBucket::new();
        let target = 160;
        for i in 0..target {
            bucket.push(i);
        }

        let mut expected = Vec::new();
        expected.extend(120..target);
        expected.extend(56..120);
        expected.extend(24..56);
        expected.extend(8..24);
        expected.extend(0..8);
        assert_eq!(bucket.data(), expected);

        // Clearing starts a stripe over from the smallest block.
        bucket.clear();
        for i in 0..20 {
            bucket.push(i);
        }
        let mut expected = Vec::new();
        expected.extend(8..20);
        expected.extend(0..8);
        assert_eq!(bucket.data(), expected);
    }

    #[test]
    fn test_bucket_block_growth() {
        let bucket = AtomicBucket::new();
        let mut block_sizes = Vec::new();
        for i in 0..(MIN_BLOCK_SIZE + 16 + 32 + MAX_BLOCK_SIZE * 2) {
            bucket.push(i);
        }
        bucket.data_with(|block| block_sizes.push(block.len()));
        assert_eq!(block_sizes, vec![MAX_BLOCK_SIZE, MAX_BLOCK_SIZE, 32, 16, MIN_BLOCK_SIZE]);
    }

    #[test]
    fn test_bucket_overaligned_values() {
        #[derive(Clone, Debug, PartialEq)]
        #[repr(align(64))]
        struct Big([u8; 3]);

        let bucket = AtomicBucket::new();
        for i in 0..MIN_BLOCK_SIZE * 4 {
            bucket.push(Big([i as u8; 3]));
        }

        let values = bucket.data();
        assert_eq!(values.len(), MIN_BLOCK_SIZE * 4);
        for value in &values {
            assert_eq!(value.0[0], value.0[1]);
            assert_eq!(value.0[0], value.0[2]);
        }
        assert_eq!(
            values.iter().map(|v| v.0[0] as usize).sum::<usize>(),
            (0..MIN_BLOCK_SIZE * 4).sum()
        );
    }

    #[test]
    fn test_bucket_write_then_read_mt() {
        const PER_WRITER: u64 =
            (if cfg!(miri) { MAX_BLOCK_SIZE * 10 } else { MAX_BLOCK_SIZE * 100_000 }) as u64;

        let bucket = AtomicBucket::new();

        let snapshot = bucket.data();
        assert_eq!(snapshot.len(), 0);

        let total = thread::scope(|s| {
            let writer = || {
                let mut total = 0;
                for i in 0..PER_WRITER {
                    bucket.push(i);
                    total += i;
                }
                total
            };
            let t1 = s.spawn(writer);
            let t2 = s.spawn(writer);
            t1.join().unwrap() + t2.join().unwrap()
        });

        let snapshot = bucket.data();
        assert_eq!(snapshot.len(), 2 * PER_WRITER as usize);

        let sum = snapshot.iter().sum::<u64>();
        assert_eq!(sum, total);
    }

    #[test]
    fn test_bucket_many_writers_with_concurrent_clears() {
        // More writers than there are stripes on any reasonable machine, all pushing while a
        // reader repeatedly clears the bucket: nothing may be lost or duplicated.
        const WRITERS: usize = if cfg!(miri) { 8 } else { 300 };
        const PER_WRITER: usize = if cfg!(miri) { 200 } else { 20_000 };
        const CLEARS: usize = if cfg!(miri) { 20 } else { 200 };

        let bucket = AtomicBucket::new();
        let barrier = Barrier::new(WRITERS + 1);
        let mut seen_total = 0;
        let mut seen_count = 0;

        let expected_total = thread::scope(|s| {
            let writers = (0..WRITERS)
                .map(|w| {
                    let (bucket, barrier) = (&bucket, &barrier);
                    s.spawn(move || {
                        barrier.wait();
                        let mut total = 0;
                        for i in 0..PER_WRITER {
                            let value = w * PER_WRITER + i;
                            bucket.push(value);
                            total += value;
                        }
                        total
                    })
                })
                .collect::<Vec<_>>();

            barrier.wait();
            for _ in 0..CLEARS {
                bucket.clear_with(|xs| {
                    seen_total += xs.iter().sum::<usize>();
                    seen_count += xs.len();
                });
                thread::yield_now();
            }

            writers.into_iter().map(|w| w.join().unwrap()).sum::<usize>()
        });

        // Pick up whatever was written after the last clear.
        bucket.clear_with(|xs| {
            seen_total += xs.iter().sum::<usize>();
            seen_count += xs.len();
        });

        assert_eq!(seen_count, WRITERS * PER_WRITER);
        assert_eq!(seen_total, expected_total);
        assert!(bucket.is_empty());
    }

    #[test]
    fn test_bucket_concurrent_readers_and_clears() {
        // Non-clearing readers must never observe freed blocks while a clearer runs, and the
        // clearer must still see every value exactly once.
        const WRITERS: usize = 4;
        const PER_WRITER: u64 = (if cfg!(miri) { 500 } else { 50_000 }) as u64;

        let bucket = AtomicBucket::new();
        let done = AtomicUsize::new(0);

        let cleared = thread::scope(|s| {
            for _ in 0..WRITERS {
                s.spawn(|| {
                    for i in 0..PER_WRITER {
                        bucket.push(i);
                    }
                    done.fetch_add(1, Ordering::Release);
                });
            }

            for _ in 0..2 {
                s.spawn(|| {
                    while done.load(Ordering::Acquire) < WRITERS {
                        bucket.data_with(|xs| {
                            for x in xs {
                                assert!(*x < PER_WRITER);
                            }
                        });
                    }
                });
            }

            let mut cleared = 0u64;
            while done.load(Ordering::Acquire) < WRITERS {
                bucket.clear_with(|xs| cleared += xs.len() as u64);
            }
            bucket.clear_with(|xs| cleared += xs.len() as u64);
            cleared
        });

        assert_eq!(cleared, WRITERS as u64 * PER_WRITER);
        assert!(bucket.is_empty());
    }

    #[test]
    fn test_clear_and_clear_with() {
        let bucket = AtomicBucket::new();

        let snapshot = bucket.data();
        assert_eq!(snapshot.len(), 0);

        let mut i = 0;
        let mut total_pushed = 0;
        while i < MAX_BLOCK_SIZE * 4 {
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
        while i < MAX_BLOCK_SIZE * 2 {
            bucket.push(i);
            assert!(!bucket.is_empty());
            i += 1;
        }
    }

    #[test]
    fn test_panicking_callbacks_do_not_wedge_bucket() {
        let bucket = AtomicBucket::new();
        for i in 0..MAX_BLOCK_SIZE * 2 {
            bucket.push(i);
        }

        // A panic while reading must still exit the stripe, so that clearing can proceed...
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bucket.data_with(|_| panic!("reader"));
        }));
        assert!(result.is_err());

        // ... and a panic while clearing must still free (and thus drop) the detached values.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bucket.clear_with(|_| panic!("clearer"));
        }));
        assert!(result.is_err());
        assert!(bucket.is_empty());

        bucket.push(42);
        assert_eq!(bucket.data(), vec![42]);
        let mut cleared = 0;
        bucket.clear_with(|xs| cleared += xs.len());
        assert_eq!(cleared, 1);
    }

    #[test]
    fn test_bucket_drops_values() {
        struct Droppable<'a>(&'a AtomicUsize);

        impl Drop for Droppable<'_> {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }

        let drops = AtomicUsize::new(0);
        let bucket = AtomicBucket::new();

        // Push from several threads so that several stripes are allocated.
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| {
                    for _ in 0..MAX_BLOCK_SIZE * 3 + 7 {
                        bucket.push(Droppable(&drops));
                    }
                });
            }
        });
        assert_eq!(drops.load(Ordering::Relaxed), 0);

        // Clearing drops the values that were cleared...
        let mut cleared = 0;
        bucket.clear_with(|xs| cleared += xs.len());
        assert_eq!(cleared, 4 * (MAX_BLOCK_SIZE * 3 + 7));
        assert_eq!(drops.load(Ordering::Relaxed), cleared);

        // ... and dropping the bucket drops whatever is left.
        bucket.push(Droppable(&drops));
        bucket.push(Droppable(&drops));
        drop(bucket);
        assert_eq!(drops.load(Ordering::Relaxed), cleared + 2);
    }
}
