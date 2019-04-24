//! Exports metrics via the `log` crate.
//!
//! This exporter can utilize recorders that are able to be converted to a textual representation
//! via [`Into`].  It will emit that output by logging via the `log` crate at the specified
//! level.
//!
//! # Run Modes
//! - `run` can be used to block the current thread, taking snapshots and exporting them on an
//! interval
//! - `turn` can be used to take a single snapshot and log it
//! - `into_future` will return a [`Future`] that when driven will take a snapshot on the 
//! configured interval and log it
#[macro_use]
extern crate log;

use std::thread;
use std::time::Duration;
use metrics::Controller;
use metrics_core::MetricsRecorder;
use log::Level;
use futures::prelude::*;
use tokio_timer::Interval;

/// Exports metrics by converting them to a textual representation and logging them.
pub struct LogExporter<R> {
    controller: Controller,
    recorder: R,
    level: Level,
}

impl<R> LogExporter<R>
where
    R: MetricsRecorder + Clone + Into<String>
{
    /// Creates a new [`LogExporter`] that logs at the configurable level.
    ///
    /// Recorders expose their output by being converted into strings.
    pub fn new(controller: Controller, recorder: R, level: Level) -> Self {
        LogExporter {
            controller,
            recorder,
            level,
        }
    }

    /// Runs this exporter on the current thread, logging output on the given interval.
    pub fn run(&mut self, interval: Duration) {
        loop {
            thread::sleep(interval);

            self.turn();
        }
    }

    /// Run this exporter, logging output only once.
    pub fn turn(&self) {
        run_once(&self.controller, self.recorder.clone(), self.level);
    }

    /// Converts this exporter into a future which logs output on the given interval.
    pub fn into_future(self, interval: Duration) -> impl Future<Item = (), Error = ()> {
        let controller = self.controller;
        let recorder = self.recorder;
        let level = self.level;

        Interval::new_interval(interval)
            .map_err(|_| ())
            .for_each(move |_| {
                let recorder = recorder.clone();
                run_once(&controller, recorder, level);
                Ok(())
            })
    }
}

fn run_once<R>(controller: &Controller, mut recorder: R, level: Level)
where
    R: MetricsRecorder + Into<String>
{
    match controller.get_snapshot() {
        Ok(snapshot) => {
            snapshot.record(&mut recorder);
            let output = recorder.into();
            log!(level, "{}", output);
        },
        Err(e) => log!(Level::Error, "failed to capture snapshot: {}", e),
    }
}
