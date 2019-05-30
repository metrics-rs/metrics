use std::error;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use metrics_core::MetricName;

#[macro_use]
mod macros;

static mut RECORDER: &'static MetricsRecorder = &NopRecorder;
static STATE: AtomicUsize = AtomicUsize::new(0);

const UNINITIALIZED: usize = 0;
const INITIALIZING: usize = 1;
const INITIALIZED: usize = 2;

static SET_RECORDER_ERROR: &'static str = "attempted to set a recorder after the metrics system was already initialized";

pub trait MetricsRecorder {
    fn record_counter(&self, key: MetricName, value: u64);
    fn record_gauge(&self, key: MetricName, value: i64);
    fn record_histogram(&self, key: MetricName, value: u64);
}

struct NopRecorder;

impl MetricsRecorder for NopRecorder {
    fn record_counter(&self, _key: MetricName, _value: u64) { }
    fn record_gauge(&self, _key: MetricName, _value: i64) { }
    fn record_histogram(&self, _key: MetricName, _value: u64) { }
}

pub fn set_boxed_recorder(recorder: Box<MetricsRecorder>) -> Result<(), SetRecorderError> {
    set_recorder_inner(|| unsafe { &*Box::into_raw(recorder) })
}

fn set_recorder_inner<F>(make_recorder: F) -> Result<(), SetRecorderError>
where
    F: FnOnce() -> &'static MetricsRecorder,
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

#[derive(Debug)]
pub struct SetRecorderError(());

impl fmt::Display for SetRecorderError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str(SET_RECORDER_ERROR)
    }
}

impl error::Error for SetRecorderError {
    fn description(&self) -> &str {
        SET_RECORDER_ERROR
    }
}

pub fn recorder() -> &'static MetricsRecorder {
    unsafe {
        if STATE.load(Ordering::SeqCst) != INITIALIZED {
            static NOP: NopRecorder = NopRecorder;
            &NOP
        } else {
            RECORDER
        }
    }
}

#[doc(hidden)]
pub fn __private_api_record_count(name: &str, value: u64) {
    recorder().record_counter(name, value);
}

#[doc(hidden)]
pub fn __private_api_record_gauge(name: &str, value: i64) {
    recorder().record_gauge(name, value);
}

#[doc(hidden)]
pub fn __private_api_record_histogram(name: &str, value: u64) {
    recorder().record_histogram(name, value);
}
