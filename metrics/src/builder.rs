use crate::Receiver;
use std::error::Error;
use std::fmt;
use std::time::Duration;

/// Errors during receiver creation.
#[derive(Debug, Clone)]
pub enum BuilderError {
    /// Failed to spawn the upkeep thread.
    ///
    /// As histograms are windowed, reads and writes require getting the current time so they can
    /// perform the required maintenance, or upkeep, on the internal structures to roll over old
    /// buckets, etc.
    ///
    /// Acquiring the current time is fast compared to most operations, but is a significant
    /// portion of the other time it takes to write to a histogram, which limits overall throughput
    /// under high load.
    ///
    /// We spin up a background thread, or the "upkeep thread", which updates a global time source
    /// that the read and write operations exclusively rely on.  While this source is not as
    /// up-to-date as the real clock, it is much faster to access.
    UpkeepFailure,
}

impl Error for BuilderError {}

impl fmt::Display for BuilderError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BuilderError::UpkeepFailure => write!(f, "failed to spawn quanta upkeep thread"),
        }
    }
}

/// Builder for [`Receiver`].
#[derive(Clone)]
pub struct Builder {
    pub(crate) histogram_window: Duration,
    pub(crate) histogram_granularity: Duration,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            histogram_window: Duration::from_secs(10),
            histogram_granularity: Duration::from_secs(1),
        }
    }
}

impl Builder {
    /// Creates a new [`Builder`] with default values.
    pub fn new() -> Self {
        Default::default()
    }

    /// Sets the histogram configuration.
    ///
    /// Defaults to a 10 second window with 1 second granularity.
    ///
    /// This controls both how long of a time window we track histogram data for, and the
    /// granularity in which we roll off old data.
    ///
    /// As an example, with the default values, we would keep the last 10 seconds worth of
    /// histogram data, and would remove 1 seconds worth of data at a time as the window rolled
    /// forward.
    pub fn histogram(mut self, window: Duration, granularity: Duration) -> Self {
        self.histogram_window = window;
        self.histogram_granularity = granularity;
        self
    }

    /// Create a [`Receiver`] based on this configuration.
    pub fn build(self) -> Result<Receiver, BuilderError> {
        Receiver::from_builder(self)
    }
}
