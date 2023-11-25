use std::{error::Error, fmt};

const SET_RECORDER_ERROR: &str =
    "attempted to set a recorder after the metrics system was already initialized";

/// The type returned by [`set_global_recorder`] if [`set_recorder`] has already been called.
pub struct SetRecorderError<R>(pub R);

impl<R> SetRecorderError<R> {
    /// Returns the recorder that was attempted to be set.
    pub fn into_inner(self) -> R {
        self.0
    }
}

impl<R> fmt::Debug for SetRecorderError<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SetRecorderError").finish_non_exhaustive()
    }
}

impl<R> fmt::Display for SetRecorderError<R> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str(SET_RECORDER_ERROR)
    }
}

impl<R> Error for SetRecorderError<R> {}
