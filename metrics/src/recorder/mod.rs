use std::{cell::Cell, ptr::NonNull};

mod cell;
use self::cell::RecorderOnceCell;

mod errors;
pub use self::errors::SetRecorderError;

mod noop;
pub use self::noop::NoopRecorder;

use crate::{Counter, Gauge, Histogram, Key, KeyName, Metadata, SharedString, Unit};

static NOOP_RECORDER: NoopRecorder = NoopRecorder;
static GLOBAL_RECORDER: RecorderOnceCell = RecorderOnceCell::new();

thread_local! {
    static LOCAL_RECORDER: Cell<Option<NonNull<dyn Recorder>>> = Cell::new(None);
}

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
    fn register_counter(&self, key: &Key, metadata: &Metadata<'_>) -> Counter;

    /// Registers a gauge.
    fn register_gauge(&self, key: &Key, metadata: &Metadata<'_>) -> Gauge;

    /// Registers a histogram.
    fn register_histogram(&self, key: &Key, metadata: &Metadata<'_>) -> Histogram;
}

/// Guard for setting a local recorder.
///
/// When using a local recorder, we take a reference to the recorder and only hold it for as long as
/// the duration of the closure. However, we must store this reference in a static variable
/// (thread-local storage) so that it can be accessed by the macros. This guard ensures that the
/// pointer we store to the reference is cleared when the guard is dropped, so that it can't be used
/// after the closure has finished, even if the closure panics and unwinds the stack.
struct LocalRecorderGuard;

impl LocalRecorderGuard {
    /// Creates a new `LocalRecorderGuard` and sets the thread-local recorder.
    fn new(recorder: &dyn Recorder) -> Self {
        // SAFETY: While we take a lifetime-less pointer to the given reference, the reference we
        // derive _from_ the pointer is never given a lifetime that exceeds the lifetime of the
        // input reference.
        let recorder_ptr = unsafe { NonNull::new_unchecked(recorder as *const _ as *mut _) };

        LOCAL_RECORDER.with(|local_recorder| {
            local_recorder.set(Some(recorder_ptr));
        });

        Self
    }
}

impl Drop for LocalRecorderGuard {
    fn drop(&mut self) {
        // Clear the thread-local recorder.
        LOCAL_RECORDER.with(|local_recorder| {
            local_recorder.set(None);
        });
    }
}

/// Sets the global recorder.
///
/// This function may only be called once in the lifetime of a program. Any metrics recorded
/// before this method is called will be completely ignored.
///
/// This function does not typically need to be called manually.  Metrics implementations should
/// provide an initialization method that installs the recorder internally.
///
/// # Errors
///
/// An error is returned if a recorder has already been set.
pub fn set_global_recorder<R>(recorder: R) -> Result<(), SetRecorderError<R>>
where
    R: Recorder + 'static,
{
    GLOBAL_RECORDER.set(recorder)
}

/// Runs the closure with the given recorder set as the global recorder for the duration.
pub fn with_local_recorder<T>(recorder: &dyn Recorder, f: impl FnOnce() -> T) -> T {
    let _local = LocalRecorderGuard::new(recorder);
    f()
}

/// Runs the closure with a reference to the current recorder for this scope.
///
/// If a local recorder has been set, it will be used. Otherwise, the global recorder will be used.
/// If neither a local recorder or global recorder have been set, a no-op recorder will be used.
///
/// This is used primarily by the generated code from the convenience macros used to record metrics.
/// It should typically not be necessary to call this function directly.
#[doc(hidden)]
pub fn with_recorder<T>(f: impl FnOnce(&dyn Recorder) -> T) -> T {
    LOCAL_RECORDER.with(|local_recorder| {
        if let Some(recorder) = local_recorder.get() {
            // SAFETY: If we have a local recorder, we know that it is valid because it can only be
            // set during the duration of a closure that is passed to `with_local_recorder`, which
            // is the only time this method can be called and have a local recorder set. This
            // ensures that the lifetime of the recorder is valid for the duration of this method
            // call.
            unsafe { f(recorder.as_ref()) }
        } else if let Some(global_recorder) = GLOBAL_RECORDER.try_load() {
            f(global_recorder)
        } else {
            f(&NOOP_RECORDER)
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    use super::{Recorder, RecorderOnceCell};

    #[test]
    fn boxed_recorder_dropped_on_existing_set() {
        // This test simply ensures that if a boxed recorder is handed to us to install, and another
        // recorder has already been installed, that we drop th new boxed recorder instead of
        // leaking it.
        struct TrackOnDropRecorder(Arc<AtomicBool>);

        impl TrackOnDropRecorder {
            pub fn new() -> (Self, Arc<AtomicBool>) {
                let arc = Arc::new(AtomicBool::new(false));
                (Self(arc.clone()), arc)
            }
        }

        impl Recorder for TrackOnDropRecorder {
            fn describe_counter(
                &self,
                _: crate::KeyName,
                _: Option<crate::Unit>,
                _: crate::SharedString,
            ) {
            }
            fn describe_gauge(
                &self,
                _: crate::KeyName,
                _: Option<crate::Unit>,
                _: crate::SharedString,
            ) {
            }
            fn describe_histogram(
                &self,
                _: crate::KeyName,
                _: Option<crate::Unit>,
                _: crate::SharedString,
            ) {
            }

            fn register_counter(&self, _: &crate::Key, _: &crate::Metadata<'_>) -> crate::Counter {
                crate::Counter::noop()
            }

            fn register_gauge(&self, _: &crate::Key, _: &crate::Metadata<'_>) -> crate::Gauge {
                crate::Gauge::noop()
            }

            fn register_histogram(
                &self,
                _: &crate::Key,
                _: &crate::Metadata<'_>,
            ) -> crate::Histogram {
                crate::Histogram::noop()
            }
        }

        impl Drop for TrackOnDropRecorder {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }

        let recorder_cell = RecorderOnceCell::new();

        // This is the first set of the cell, so it should always succeed.
        let (first_recorder, _) = TrackOnDropRecorder::new();
        let first_set_result = recorder_cell.set(first_recorder);
        assert!(first_set_result.is_ok());

        // Since the cell is already set, this second set should fail. We'll also then assert that
        // our atomic boolean is set to `true`, indicating the drop logic ran for it.
        let (second_recorder, was_dropped) = TrackOnDropRecorder::new();
        assert!(!was_dropped.load(Ordering::SeqCst));

        let second_set_result = recorder_cell.set(second_recorder);
        assert!(second_set_result.is_err());
        assert!(!was_dropped.load(Ordering::SeqCst));
        drop(second_set_result);
        assert!(was_dropped.load(Ordering::SeqCst));
    }
}
