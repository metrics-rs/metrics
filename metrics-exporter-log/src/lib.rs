//! Exports metrics via the `log` crate.
//!
//! This exporter can utilize recorders that are able to be converted to a textual representation
//! via [`Into`].  It will emit that output by logging via the `log` crate at the specified
//! level.
//!
//! # Run Modes
//! - `run` can be used to block the current thread, taking snapshots and exporting them on an
//! interval
//! - `into_future` will return a [`Future`] that when driven will take a snapshot on the
//! configured interval and log it
#[macro_use]
extern crate log;

use futures::prelude::*;
use log::Level;
use metrics_core::{AsyncSnapshotProvider, Recorder, Snapshot, SnapshotProvider};
use std::error::Error;
use std::thread;
use std::time::Duration;
use tokio_timer::Interval;

/// Exports metrics by converting them to a textual representation and logging them.
pub struct LogExporter<C, R> {
    controller: C,
    recorder: R,
    level: Level,
}

impl<C, R> LogExporter<C, R>
where
    R: Recorder + Clone + Into<String>,
{
    /// Creates a new [`LogExporter`] that logs at the configurable level.
    ///
    /// Recorders expose their output by being converted into strings.
    pub fn new(controller: C, recorder: R, level: Level) -> Self {
        LogExporter {
            controller,
            recorder,
            level,
        }
    }

    /// Runs this exporter on the current thread, logging output on the given interval.
    pub fn run(&mut self, interval: Duration)
    where
        C: SnapshotProvider,
        C::SnapshotError: Error,
    {
        loop {
            thread::sleep(interval);

            self.turn();
        }
    }

    /// Run this exporter, logging output only once.
    pub fn turn(&self)
    where
        C: SnapshotProvider,
        C::SnapshotError: Error,
    {
        match self.controller.get_snapshot() {
            Ok(snapshot) => {
                let mut recorder = self.recorder.clone();
                snapshot.record(&mut recorder);
                let output = recorder.into();
                log!(self.level, "{}", output);
            }
            Err(e) => log!(Level::Error, "failed to get snapshot: {}", e),
        }
    }

    /// Converts this exporter into a future which logs output on the given interval.
    pub fn into_future(self, interval: Duration) -> impl Future<Item = (), Error = ()>
    where
        C: AsyncSnapshotProvider,
        C::SnapshotError: Error,
    {
        let controller = self.controller;
        let recorder = self.recorder;
        let level = self.level;

        Interval::new_interval(interval)
            .map_err(|_| ())
            .for_each(move |_| {
                let mut recorder = recorder.clone();

                controller
                    .get_snapshot_async()
                    .and_then(move |snapshot| {
                        snapshot.record(&mut recorder);
                        let output = recorder.into();
                        log!(level, "{}", output);
                        Ok(())
                    })
                    .map_err(|e| error!("failed to get snapshot: {}", e))
            })
    }
}
