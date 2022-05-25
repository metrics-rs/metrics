use self::cell::RecorderOnceCell;
use crate::{Counter, Gauge, Histogram, Key, KeyName, Unit};
use core::fmt;

mod cell {
    use super::{Recorder, SetRecorderError};
    use std::cell::UnsafeCell;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // FIXME: This can't be a const new function because trait objects aren't allowed in const fns
    // This was stabilized in 1.61, so it can be cleaned up when it becomes the MSRV
    #[allow(clippy::declare_interior_mutable_const)]
    pub const INIT: RecorderOnceCell = RecorderOnceCell {
        recorder: UnsafeCell::new(None),
        state: AtomicUsize::new(UNINITIALIZED),
    };

    /// The global Recorder instance with a `once_cell`-like API.
    pub struct RecorderOnceCell {
        recorder: UnsafeCell<Option<&'static dyn Recorder>>,
        state: AtomicUsize,
    }

    /// The recorder is uninit and can be set.
    const UNINITIALIZED: usize = 0;
    /// The recorder is currently being initialized.
    const INITIALIZING: usize = 1;
    /// The recorder has been initialized successfully and can be read.
    const INITIALIZED: usize = 2;

    impl RecorderOnceCell {
        #[cfg(atomic_cas)]
        pub fn set(&self, recorder: &'static dyn Recorder) -> Result<(), SetRecorderError> {
            // Acquire the lock because the write below must not be reordered above the CAS.
            match self.state.compare_exchange(
                UNINITIALIZED,
                INITIALIZING,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(UNINITIALIZED) => {
                    unsafe {
                        // SAFETY: Access is unique because we CASed the state to INITIALIZING above
                        self.recorder.get().write(Some(recorder));
                    }
                    // Release the lock, others can now read it - but not write
                    self.state.store(INITIALIZED, Ordering::Release);
                    Ok(())
                }
                _ => Err(SetRecorderError(())),
            }
        }

        /// Clears the currently installed recorder, allowing a new writer to override it.
        /// # Safety
        /// The caller must guarantee that no reader has read the state before we do this and then
        /// reads the recorder after another writer has written to it after us.
        pub unsafe fn clear(&self) {
            // Set the state to `UNINIT` to allow the next writer to write again.
            // This is not a problem for readers since their `&'static` refs will remain
            // valid forever.
            self.state.store(UNINITIALIZED, Ordering::Relaxed);
        }

        pub fn try_load(&self) -> Option<&'static dyn Recorder> {
            if self.state.load(Ordering::Acquire) != INITIALIZED {
                None
            } else {
                // SAFETY: Thanks to `Acquire` above we make sure that this doesn't get
                // reordered above this and therefore no writer is here
                unsafe { self.recorder.get().read() }
            }
        }

        pub unsafe fn set_racy(
            &self,
            recorder: &'static dyn Recorder,
        ) -> Result<(), SetRecorderError> {
            match self.state.load(Ordering::Relaxed) {
                UNINITIALIZED => {
                    // SAFETY: Caller guarantees that access is unique
                    self.recorder.get().write(Some(recorder));
                    self.state.store(INITIALIZED, Ordering::Release);
                    Ok(())
                }
                INITIALIZING => {
                    // This is just plain UB, since we were racing another initialization function
                    unreachable!(
                        "set_recorder_racy must not be used with other initialization functions"
                    )
                }
                _ => Err(SetRecorderError(())),
            }
        }
    }

    // SAFETY: We can only mutate through `set` - which is protected by the `state` and unsafe
    // function where the caller has to guarantee synced-ness
    unsafe impl Send for RecorderOnceCell {}
    unsafe impl Sync for RecorderOnceCell {}
}

static RECORDER: RecorderOnceCell = cell::INIT;

static SET_RECORDER_ERROR: &str =
    "attempted to set a recorder after the metrics system was already initialized";

/// A trait for registering and recording metrics.
///
/// This is the core trait that allows interoperability between exporter implementations and the
/// macros provided by `metrics`.
pub trait Recorder {
    /// Describes a counter.
    ///
    /// Callers may provide the unit or a description of the counter being registered. Whether or
    /// not a metric can be reregistered to provide a unit/description, if one was already passed
    /// or not, as well as how units/descriptions are used by the underlying recorder, is an
    /// implementation detail.
    fn describe_counter(&self, key: KeyName, unit: Option<Unit>, description: &'static str);

    /// Describes a gauge.
    ///
    /// Callers may provide the unit or a description of the gauge being registered. Whether or
    /// not a metric can be reregistered to provide a unit/description, if one was already passed
    /// or not, as well as how units/descriptions are used by the underlying recorder, is an
    /// implementation detail.
    fn describe_gauge(&self, key: KeyName, unit: Option<Unit>, description: &'static str);

    /// Describes a histogram.
    ///
    /// Callers may provide the unit or a description of the histogram being registered. Whether or
    /// not a metric can be reregistered to provide a unit/description, if one was already passed
    /// or not, as well as how units/descriptions are used by the underlying recorder, is an
    /// implementation detail.
    fn describe_histogram(&self, key: KeyName, unit: Option<Unit>, description: &'static str);

    /// Registers a counter.
    fn register_counter(&self, key: &Key) -> Counter;

    /// Registers a gauge.
    fn register_gauge(&self, key: &Key) -> Gauge;

    /// Registers a histogram.
    fn register_histogram(&self, key: &Key) -> Histogram;
}

/// A no-op recorder.
///
/// Used as the default recorder when one has not been installed yet.  Useful for acting as the root
/// recorder when testing layers.
pub struct NoopRecorder;

impl Recorder for NoopRecorder {
    fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: &'static str) {}
    fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: &'static str) {}
    fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: &'static str) {}
    fn register_counter(&self, _key: &Key) -> Counter {
        Counter::noop()
    }
    fn register_gauge(&self, _key: &Key) -> Gauge {
        Gauge::noop()
    }
    fn register_histogram(&self, _key: &Key) -> Histogram {
        Histogram::noop()
    }
}

/// Sets the global recorder to a `&'static Recorder`.
///
/// This function may only be called once in the lifetime of a program.  Any metrics recorded
/// before the call to `set_recorder` occurs will be completely ignored.
///
/// This function does not typically need to be called manually.  Metrics implementations should
/// provide an initialization method that installs the recorder internally.
///
/// # Errors
///
/// An error is returned if a recorder has already been set.
#[cfg(atomic_cas)]
pub fn set_recorder(recorder: &'static dyn Recorder) -> Result<(), SetRecorderError> {
    RECORDER.set(recorder)
}

/// Sets the global recorder to a `Box<Recorder>`.
///
/// This is a simple convenience wrapper over `set_recorder`, which takes a `Box<Recorder>`
/// rather than a `&'static Recorder`.  See the document for [`set_recorder`] for more
/// details.
///
/// Requires the `std` feature.
///
/// # Errors
///
/// An error is returned if a recorder has already been set.
#[cfg(atomic_cas)]
pub fn set_boxed_recorder(recorder: Box<dyn Recorder>) -> Result<(), SetRecorderError> {
    RECORDER.set(Box::leak(recorder))
}

/// A thread-unsafe version of [`set_recorder`].
///
/// This function is available on all platforms, even those that do not have support for atomics
/// that are needed by [`set_recorder`].
///
/// In almost all cases, [`set_recorder`] should be preferred.
///
/// # Safety
///
/// This function is only safe to call when no other metrics initialization function is called
/// while this function still executes.
///
/// This can be upheld by (for example) making sure that **there are no other threads**, and (on
/// embedded) that **interrupts are disabled**.
///
/// It is safe to use other metrics functions while this function runs (including all metrics
/// macros).
pub unsafe fn set_recorder_racy(recorder: &'static dyn Recorder) -> Result<(), SetRecorderError> {
    RECORDER.set_racy(recorder)
}

/// Clears the currently configured recorder.
///
/// As we give out a reference to the recorder with a static lifetime, we cannot safely reclaim
/// and drop the installed recorder when clearing.  Thus, any existing recorder will stay leaked.
///
/// This method is typically only useful for testing or benchmarking.
///
/// # Safety
///
/// This function must not be called during any readers reading or writers writing.
/// The caller can cause readers and writers to race if they are in reading/writing while
/// this function is called.
#[doc(hidden)]
pub unsafe fn clear_recorder() {
    RECORDER.clear();
}

/// The type returned by [`set_recorder`] if [`set_recorder`] has already been called.
#[derive(Debug)]
pub struct SetRecorderError(());

impl fmt::Display for SetRecorderError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str(SET_RECORDER_ERROR)
    }
}

// The Error trait is not available in libcore
impl std::error::Error for SetRecorderError {
    fn description(&self) -> &str {
        SET_RECORDER_ERROR
    }
}

/// Returns a reference to the recorder.
///
/// If a recorder has not been set, a no-op implementation is returned.
pub fn recorder() -> &'static dyn Recorder {
    static NOOP: NoopRecorder = NoopRecorder;
    try_recorder().unwrap_or(&NOOP)
}

/// Returns a reference to the recorder.
///
/// If a recorder has not been set, returns `None`.
pub fn try_recorder() -> Option<&'static dyn Recorder> {
    RECORDER.try_load()
}
