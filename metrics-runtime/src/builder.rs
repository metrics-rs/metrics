use crate::{config::Configuration, Receiver};
use std::{error::Error, fmt, time::Duration};

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

    #[doc(hidden)]
    _NonExhaustive,
}

impl Error for BuilderError {}

impl fmt::Display for BuilderError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BuilderError::UpkeepFailure => write!(f, "failed to spawn quanta upkeep thread"),
            BuilderError::_NonExhaustive => write!(f, "non-exhaustive matching"),
        }
    }
}

/// Builder for [`Receiver`].
#[derive(Clone)]
pub struct Builder {
    pub(crate) histogram_window: Duration,
    pub(crate) histogram_granularity: Duration,
    pub(crate) upkeep_interval: Duration,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            histogram_window: Duration::from_secs(10),
            histogram_granularity: Duration::from_secs(1),
            upkeep_interval: Duration::from_millis(50),
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

    /// Sets the upkeep interval.
    ///
    /// Defaults to 50 milliseconds.
    ///
    /// This controls how often the time source, used internally by histograms, is updated with the
    /// real time.  For performance reasons, histograms use a sampled time source when they perform
    /// checks to see if internal maintenance needs to occur.  If the histogram granularity is set
    /// very low, then this interval might need to be similarly reduced to make sure we're able to
    /// update the time more often than histograms need to perform upkeep.
    pub fn upkeep_interval(mut self, interval: Duration) -> Self {
        self.upkeep_interval = interval;
        self
    }

    /// Create a [`Receiver`] based on this configuration.
    pub fn build(self) -> Result<Receiver, BuilderError> {
        let config = Configuration::from_builder(&self);
        Receiver::from_config(config)
    }
}
