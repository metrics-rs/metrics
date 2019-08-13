//! A lightweight metrics facade.
//!
//! The `metrics` crate provides a single metrics API that abstracts over the actual metrics
//! implementation.  Libraries can use the metrics API provided by this crate, and the consumer of
//! those libraries can choose the metrics implementation that is most suitable for its use case.
//!
//! If no metrics implementation is selected, the facade falls back to a "noop" implementation that
//! ignores all metrics.  The overhead in this case is very small - an atomic load and comparison.
//!
//! # Use
//! The basic use of the facade crate is through the four metrics macros: [`counter!`], [`gauge!`],
//! [`timing!`], and [`value!`].  These macros correspond to updating a counter, updating a gauge,
//! updating a histogram based on a start/end, and updating a histogram with a single value.
//!
//! Both [`timing!`] and [`value!`] are effectively identical in so far as that they both translate
//! to recording a single value to an underlying histogram, but [`timing!`] is provided for
//! contextual consistency: if you're recording a measurement of the time passed during an
//! operation, the end result is a single value, but it's more of a "timing" value than just a
//! "value".  The [`timing!`] macro also has a branch to accept the start and end values which
//! allows for a potentially clearer invocation.
//!
//! ## In libraries
//! Libraries should link only to the `metrics` crate, and use the provided macros to record
//! whatever metrics will be useful to downstream consumers.
//!
//! ### Examples
//!
//! ```rust
//! use metrics::{timing, counter};
//!
//! # use std::time::Instant;
//! # pub fn run_query(_: &str) -> u64 { 42 }
//! pub fn process(query: &str) -> u64 {
//!     let start = Instant::now();
//!     let row_count = run_query(query);
//!     let end = Instant::now();
//!
//!     timing!("process.query_time", start, end);
//!     counter!("process.query_row_count", row_count);
//!
//!     row_count
//! }
//! # fn main() {}
//! ```
//!
//! ## In executables
//!
//! Executables should choose a metrics implementation and initialize it early in the runtime of
//! the program.  Metrics implementations will typically include a function to do this.  Any
//! metrics recordered before the implementation is initialized will be ignored.
//!
//! The executable itself may use the `metrics` crate to record metrics well.
//!
//! ### Warning
//!
//! The metrics system may only be initialized once.
//!
//! # Available metrics implementations
//!
//! * # Native recorder:
//!     * [metrics-runtime]
//!
//! # Implementing a Recorder
//!
//! Recorders implement the [`Recorder`] trait.  Here's a basic example which writes the
//! metrics in text form via the `log` crate.
//!
//! ```rust
//! use log::info;
//! use metrics::Recorder;
//! use metrics_core::Key;
//!
//! struct LogRecorder;
//!
//! impl Recorder for LogRecorder {
//!     fn record_counter(&self, key: Key, value: u64) {
//!         info!("counter '{}' -> {}", key, value);
//!     }
//!
//!     fn record_gauge(&self, key: Key, value: i64) {
//!         info!("gauge '{}' -> {}", key, value);
//!     }
//!
//!     fn record_histogram(&self, key: Key, value: u64) {
//!         info!("histogram '{}' -> {}", key, value);
//!     }
//! }
//! # fn main() {}
//! ```
//!
//! Recorders are installed by calling the [`set_recorder`] function.  Recorders should provide a
//! function that wraps the creation and installation of the recorder:
//!
//! ```rust
//! # use metrics::Recorder;
//! # use metrics_core::Key;
//! # struct LogRecorder;
//! # impl Recorder for LogRecorder {
//! #     fn record_counter(&self, _key: Key, _value: u64) {}
//! #     fn record_gauge(&self, _key: Key, _value: i64) {}
//! #     fn record_histogram(&self, _key: Key, _value: u64) {}
//! # }
//! use metrics::SetRecorderError;
//!
//! static RECORDER: LogRecorder = LogRecorder;
//!
//! pub fn init() -> Result<(), SetRecorderError> {
//!     metrics::set_recorder(&RECORDER)
//! }
//! # fn main() {}
//! ```
//!
//! # Use with `std`
//!
//! `set_recorder` requires you to provide a `&'static Recorder`, which can be hard to
//! obtain if your recorder depends on some runtime configuration.  The `set_boxed_recorder`
//! function is available with the `std` Cargo feature.  It is identical to `set_recorder` except
//! that it takes a `Box<Recorder>` rather than a `&'static Recorder`:
//!
//! ```rust
//! # use metrics::Recorder;
//! # use metrics_core::Key;
//! # struct LogRecorder;
//! # impl Recorder for LogRecorder {
//! #     fn record_counter(&self, _key: Key, _value: u64) {}
//! #     fn record_gauge(&self, _key: Key, _value: i64) {}
//! #     fn record_histogram(&self, _key: Key, _value: u64) {}
//! # }
//! use metrics::SetRecorderError;
//!
//! # #[cfg(feature = "std")]
//! pub fn init() -> Result<(), SetRecorderError> {
//!     metrics::set_boxed_recorder(Box::new(LogRecorder))
//! }
//! # fn main() {}
//! ```
//!
//! [metrics-runtime]: https://docs.rs/metrics-runtime
#![deny(missing_docs)]
use metrics_core::AsNanoseconds;
pub use metrics_core::{labels, Key, Label};
#[cfg(feature = "std")]
use std::error;
use std::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
};

#[macro_use]
mod macros;

static mut RECORDER: &'static dyn Recorder = &NoopRecorder;
static STATE: AtomicUsize = AtomicUsize::new(0);

const UNINITIALIZED: usize = 0;
const INITIALIZING: usize = 1;
const INITIALIZED: usize = 2;

static SET_RECORDER_ERROR: &str =
    "attempted to set a recorder after the metrics system was already initialized";

/// A value that records metrics behind the facade.
pub trait Recorder {
    /// Records a counter.
    ///
    /// From the perspective of an recorder, a counter and gauge are essentially identical, insofar
    /// as they are both a single value tied to a key.  From the perspective of a collector,
    /// counters and gauges usually have slightly different modes of operation.
    ///
    /// For the sake of flexibility on the exporter side, both are provided.
    fn record_counter(&self, key: Key, value: u64);

    /// Records a gauge.
    ///
    /// From the perspective of a recorder, a counter and gauge are essentially identical, insofar
    /// as they are both a single value tied to a key.  From the perspective of a collector,
    /// counters and gauges usually have slightly different modes of operation.
    ///
    /// For the sake of flexibility on the exporter side, both are provided.
    fn record_gauge(&self, key: Key, value: i64);

    /// Records a histogram.
    ///
    /// Recorders are expected to tally their own histogram views, so this will be called with all
    /// of the underlying observed values, and callers will need to process them accordingly.
    ///
    /// There is no guarantee that this method will not be called multiple times for the same key.
    fn record_histogram(&self, key: Key, value: u64);
}

struct NoopRecorder;

impl Recorder for NoopRecorder {
    fn record_counter(&self, _key: Key, _value: u64) {}
    fn record_gauge(&self, _key: Key, _value: i64) {}
    fn record_histogram(&self, _key: Key, _value: u64) {}
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
    set_recorder_inner(|| recorder)
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
#[cfg(all(feature = "std", atomic_cas))]
pub fn set_boxed_recorder(recorder: Box<dyn Recorder>) -> Result<(), SetRecorderError> {
    set_recorder_inner(|| unsafe { &*Box::into_raw(recorder) })
}

#[cfg(atomic_cas)]
fn set_recorder_inner<F>(make_recorder: F) -> Result<(), SetRecorderError>
where
    F: FnOnce() -> &'static dyn Recorder,
{
    unsafe {
        match STATE.compare_and_swap(UNINITIALIZED, INITIALIZING, Ordering::SeqCst) {
            UNINITIALIZED => {
                RECORDER = make_recorder();
                STATE.store(INITIALIZED, Ordering::SeqCst);
                Ok(())
            }
            INITIALIZING => {
                while STATE.load(Ordering::SeqCst) == INITIALIZING {}
                Err(SetRecorderError(()))
            }
            _ => Err(SetRecorderError(())),
        }
    }
}

/// A thread-unsafe version of [`set_recorder`].
///
/// This function is available on all platforms, even those that do not have support for atomics
/// that is need by [`set_recorder`].
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
    match STATE.load(Ordering::SeqCst) {
        UNINITIALIZED => {
            RECORDER = recorder;
            STATE.store(INITIALIZED, Ordering::SeqCst);
            Ok(())
        }
        INITIALIZING => {
            // This is just plain UB, since we were racing another initialization function
            unreachable!("set_recorder_racy must not be used with other initialization functions")
        }
        _ => Err(SetRecorderError(())),
    }
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
#[cfg(feature = "std")]
impl error::Error for SetRecorderError {
    fn description(&self) -> &str {
        SET_RECORDER_ERROR
    }
}

/// Returns a reference to the recorder.
///
/// If a recorder has not been set, a no-op implementation is returned.
pub fn recorder() -> &'static dyn Recorder {
    unsafe {
        if STATE.load(Ordering::SeqCst) != INITIALIZED {
            static NOOP: NoopRecorder = NoopRecorder;
            &NOOP
        } else {
            RECORDER
        }
    }
}

#[doc(hidden)]
pub fn __private_api_record_count(key: Key, value: u64) {
    recorder().record_counter(key, value);
}

#[doc(hidden)]
pub fn __private_api_record_gauge<K: Into<Key>>(key: K, value: i64) {
    recorder().record_gauge(key.into(), value);
}

#[doc(hidden)]
pub fn __private_api_record_histogram<K: Into<Key>, V: AsNanoseconds>(key: K, value: V) {
    recorder().record_histogram(key.into(), value.as_nanos());
}
