use std::fmt;

use self::cell::RecorderOnceCell;
use crate::{Counter, Gauge, Histogram, Key, KeyName, SharedString, Unit};

mod cell {
    use super::{Recorder, SetRecorderError};
    use std::{
        cell::UnsafeCell,
        sync::atomic::{AtomicUsize, Ordering},
    };

    /// The recorder is uninitialized.
    const UNINITIALIZED: usize = 0;

    /// The recorder is currently being initialized.
    const INITIALIZING: usize = 1;

    /// The recorder has been initialized successfully and can be read.
    const INITIALIZED: usize = 2;

    /// An specialized version of `OnceCell` for `Recorder`.
    pub struct RecorderOnceCell {
        recorder: UnsafeCell<Option<&'static dyn Recorder>>,
        state: AtomicUsize,
    }

    impl RecorderOnceCell {
        /// Creates an uninitialized `RecorderOnceCell`.
        pub const fn new() -> Self {
            Self { recorder: UnsafeCell::new(None), state: AtomicUsize::new(UNINITIALIZED) }
        }

        pub fn set(&self, recorder: &'static dyn Recorder) -> Result<(), SetRecorderError> {
            // Try and transition the cell from `UNINITIALIZED` to `INITIALIZING`, which would give
            // us exclusive access to set the recorder.
            match self.state.compare_exchange(
                UNINITIALIZED,
                INITIALIZING,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(UNINITIALIZED) => {
                    unsafe {
                        // SAFETY: Access is unique because we can only be here if we won the race
                        // to transition from `UNINITIALIZED` to `INITIALIZING` above.
                        self.recorder.get().write(Some(recorder));
                    }

                    // Mark the recorder as initialized, which will make it visible to readers.
                    self.state.store(INITIALIZED, Ordering::Release);
                    Ok(())
                }
                _ => Err(SetRecorderError(())),
            }
        }

        /// Clears the currently installed recorder, allowing a new writer to override it.
        ///
        /// # Safety
        ///
        /// The caller must guarantee that no reader has read the state before we do this and then
        /// reads the recorder after another writer has written to it after us.
        pub unsafe fn clear(&self) {
            // Set the state to `UNINITIALIZED` to allow the next writer to write again. This is not
            // a problem for readers since their `&'static` refs will remain valid forever.
            self.state.store(UNINITIALIZED, Ordering::Relaxed);
        }

        pub fn try_load(&self) -> Option<&'static dyn Recorder> {
            if self.state.load(Ordering::Acquire) != INITIALIZED {
                None
            } else {
                // SAFETY: If the state is `INITIALIZED`, then we know that the recorder has been
                // installed and is safe to read.
                unsafe { self.recorder.get().read() }
            }
        }
    }

    // SAFETY: We can only mutate through `set`, which is protected by the `state` and unsafe
    // function where the caller has to guarantee synced-ness.
    unsafe impl Send for RecorderOnceCell {}
    unsafe impl Sync for RecorderOnceCell {}
}

static RECORDER: RecorderOnceCell = RecorderOnceCell::new();

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
    fn describe_counter(&self, key: KeyName, unit: Option<Unit>, description: SharedString);

    /// Describes a gauge.
    ///
    /// Callers may provide the unit or a description of the gauge being registered. Whether or
    /// not a metric can be reregistered to provide a unit/description, if one was already passed
    /// or not, as well as how units/descriptions are used by the underlying recorder, is an
    /// implementation detail.
    fn describe_gauge(&self, key: KeyName, unit: Option<Unit>, description: SharedString);

    /// Describes a histogram.
    ///
    /// Callers may provide the unit or a description of the histogram being registered. Whether or
    /// not a metric can be reregistered to provide a unit/description, if one was already passed
    /// or not, as well as how units/descriptions are used by the underlying recorder, is an
    /// implementation detail.
    fn describe_histogram(&self, key: KeyName, unit: Option<Unit>, description: SharedString);

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
    fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}
    fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}
    fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}
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
pub fn set_recorder(recorder: &'static dyn Recorder) -> Result<(), SetRecorderError> {
    RECORDER.set(recorder)
}

/// Sets the global recorder to a `Box<Recorder>`.
///
/// This is a simple convenience wrapper over `set_recorder`, which takes a `Box<Recorder>`
/// rather than a `&'static Recorder`.  See the documentation for [`set_recorder`] for more
/// details.
///
/// # Errors
///
/// An error is returned if a recorder has already been set.
pub fn set_boxed_recorder(recorder: Box<dyn Recorder>) -> Result<(), SetRecorderError> {
    RECORDER.set(Box::leak(recorder))
}

/// Clears the currently configured recorder.
///
/// This will leak the currently installed recorder, as we cannot safely drop it due to it being
/// provided via a reference with a `'static` lifetime.
///
/// This method is typically only useful for testing or benchmarking.
///
/// # Safety
///
/// The caller must ensure that this method is not being called while other threads are either
/// loading a reference to the global recorder, or attempting to initialize the global recorder, as
/// it can cause a data race.
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
