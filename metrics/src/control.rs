use super::data::snapshot::Snapshot;
use crossbeam_channel::{bounded, Sender};
use futures::prelude::*;
use metrics_core::{AsyncSnapshotProvider, SnapshotProvider};
use std::error::Error;
use std::fmt;
use tokio_sync::oneshot;

/// Error conditions when retrieving a snapshot.
#[derive(Debug, Clone)]
pub enum SnapshotError {
    /// There was an internal error when trying to collect a snapshot.
    InternalError,

    /// A snapshot was requested but the receiver is shutdown.
    ReceiverShutdown,
}

impl Error for SnapshotError {}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SnapshotError::InternalError => write!(f, "internal error while collecting snapshot"),
            SnapshotError::ReceiverShutdown => write!(f, "receiver is shutdown"),
        }
    }
}

/// Various control actions performed by a controller.
pub(crate) enum ControlFrame {
    /// Takes a snapshot of the current metric state.
    Snapshot(Sender<Snapshot>),

    /// Takes a snapshot of the current metric state, but uses an asynchronous channel.
    SnapshotAsync(oneshot::Sender<Snapshot>),
}

/// Dedicated handle for performing operations on a running [`Receiver`](crate::receiver::Receiver).
///
/// The caller is able to request metric snapshots at any time without requiring mutable access to
/// the sink.  This all flows through the existing control mechanism, and so is very fast.
#[derive(Clone)]
pub struct Controller {
    control_tx: Sender<ControlFrame>,
}

impl Controller {
    pub(crate) fn new(control_tx: Sender<ControlFrame>) -> Controller {
        Controller { control_tx }
    }
}

impl SnapshotProvider for Controller {
    type Snapshot = Snapshot;
    type SnapshotError = SnapshotError;

    /// Gets a snapshot.
    fn get_snapshot(&self) -> Result<Snapshot, SnapshotError> {
        let (tx, rx) = bounded(0);
        let msg = ControlFrame::Snapshot(tx);

        self.control_tx
            .send(msg)
            .map_err(|_| SnapshotError::ReceiverShutdown)
            .and_then(move |_| rx.recv().map_err(|_| SnapshotError::InternalError))
    }
}

impl AsyncSnapshotProvider for Controller {
    type Snapshot = Snapshot;
    type SnapshotError = SnapshotError;
    type SnapshotFuture = SnapshotFuture;

    /// Gets a snapshot asynchronously.
    fn get_snapshot_async(&self) -> Self::SnapshotFuture {
        let (tx, rx) = oneshot::channel();
        let msg = ControlFrame::SnapshotAsync(tx);

        self.control_tx
            .send(msg)
            .map(move |_| SnapshotFuture::Waiting(rx))
            .unwrap_or(SnapshotFuture::Errored(SnapshotError::ReceiverShutdown))
    }
}

/// A future representing collecting a snapshot.
pub enum SnapshotFuture {
    Waiting(oneshot::Receiver<Snapshot>),
    Errored(SnapshotError),
}

impl Future for SnapshotFuture {
    type Item = Snapshot;
    type Error = SnapshotError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self {
            SnapshotFuture::Waiting(rx) => rx.poll().map_err(|_| SnapshotError::InternalError),
            SnapshotFuture::Errored(err) => Err(err.clone()),
        }
    }
}
