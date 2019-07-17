//! Exports metrics via the `log` crate.
//!
//! This exporter can utilize observers that are able to be converted to a textual representation
//! via [`Drain<String>`].  It will emit that output by logging via the `log` crate at the specified
//! level.
//!
//! # Run Modes
//! - `run` can be used to block the current thread, taking snapshots and exporting them on an
//! interval
//! - `into_future` will return a [`Future`] that when driven will take a snapshot on the
//! configured interval and log it
#![deny(missing_docs)]
#[macro_use]
extern crate log;

use futures::prelude::*;
use log::Level;
use metrics_core::{Builder, Drain, Observe, Observer};
use std::{thread, time::Duration};
use tokio_timer::Interval;

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

    /// Converts this exporter into a future which logs output at the intervel
    /// given on construction.
    pub fn into_future(mut self) -> impl Future<Item = (), Error = ()> {
        Interval::new_interval(self.interval)
            .map_err(|_| ())
            .for_each(move |_| {
                self.turn();
                Ok(())
            })
    }
}
