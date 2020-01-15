//! Exports metrics via the `log` crate.
//!
//! This exporter can utilize observers that are able to be converted to a textual representation
//! via [`Drain<String>`].  It will emit that output by logging via the `log` crate at the specified
//! level.
//!
//! # Run Modes
//! - Using `run` will block the current thread, capturing a snapshot and logging it based on the
//! configured interval.
//! - Using `async_run` will return a future that can be awaited on, mimicing the behavior of
//! `run`.
#![deny(missing_docs)]
#[macro_use]
extern crate log;

use log::Level;
use metrics_core::{Builder, Drain, Observe, Observer};
use std::{thread, time::Duration};
use tokio::time;

/// Exports metrics by converting them to a textual representation and logging them.
pub struct LogExporter<C, B>
where
    B: Builder,
{
    controller: C,
    observer: B::Output,
    level: Level,
    interval: Duration,
}

impl<C, B> LogExporter<C, B>
where
    B: Builder,
    B::Output: Drain<String> + Observer,
    C: Observe,
{
    /// Creates a new [`LogExporter`] that logs at the configurable level.
    ///
    /// Observers expose their output by being converted into strings.
    pub fn new(controller: C, builder: B, level: Level, interval: Duration) -> Self {
        LogExporter {
            controller,
            observer: builder.build(),
            level,
            interval,
        }
    }

    /// Runs this exporter on the current thread, logging output at the interval
    /// given on construction.
    pub fn run(&mut self) {
        loop {
            thread::sleep(self.interval);

            self.turn();
        }
    }

    /// Run this exporter, logging output only once.
    pub fn turn(&mut self) {
        self.controller.observe(&mut self.observer);
        let output = self.observer.drain();
        log!(self.level, "{}", output);
    }

    /// Converts this exporter into a future which logs output at the interval
    /// given on construction.
    pub async fn async_run(mut self) {
        let mut interval = time::interval(self.interval);
        loop {
            interval.tick().await;
            self.turn();
        }
    }
}
