use crate::data::Snapshot;
use crate::registry::{MetricRegistry, ScopeRegistry};
use futures::prelude::*;
use metrics_core::{AsyncSnapshotProvider, SnapshotProvider};
use std::error::Error;
use std::fmt;
use std::sync::Arc;

/// Error during snapshot retrieval.
#[derive(Debug, Clone)]
pub enum SnapshotError {
    /// The future was polled again after returning the snapshot.
    AlreadyUsed,
}

impl Error for SnapshotError {}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SnapshotError::AlreadyUsed => write!(f, "snapshot already returned from future"),
        }
    }
}

/// Handle for acquiring snapshots.
///
/// `Controller` is [`metrics-core`]-compatible as a snapshot provider, both for synchronous and
/// asynchronous snapshotting.
///
/// [`metrics-core`]: https://docs.rs/metrics-core
#[derive(Clone)]
pub struct Controller {
    metric_registry: Arc<MetricRegistry>,
    scope_registry: Arc<ScopeRegistry>,
}

impl Controller {
    pub(crate) fn new(
        metric_registry: Arc<MetricRegistry>,
        scope_registry: Arc<ScopeRegistry>,
    ) -> Controller {
        Controller {
            metric_registry,
            scope_registry,
        }
    }
}

impl SnapshotProvider for Controller {
    type Snapshot = Snapshot;
    type SnapshotError = SnapshotError;

    /// Gets a snapshot.
    fn get_snapshot(&self) -> Result<Snapshot, SnapshotError> {
        let snapshot = self.metric_registry.get_snapshot();
        Ok(snapshot)
    }
}

impl AsyncSnapshotProvider for Controller {
    type Snapshot = Snapshot;
    type SnapshotError = SnapshotError;
    type SnapshotFuture = SnapshotFuture;

    /// Gets a snapshot asynchronously.
    fn get_snapshot_async(&self) -> Self::SnapshotFuture {
        let snapshot = self.metric_registry.get_snapshot();
        SnapshotFuture::new(snapshot)
    }
}

/// A future representing collecting a snapshot.
pub struct SnapshotFuture {
    snapshot: Option<Snapshot>,
}

impl SnapshotFuture {
    pub fn new(snapshot: Snapshot) -> Self {
        SnapshotFuture {
            snapshot: Some(snapshot),
        }
    }
}

impl Future for SnapshotFuture {
    type Item = Snapshot;
    type Error = SnapshotError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.snapshot
            .take()
            .ok_or(SnapshotError::AlreadyUsed)
            .map(Async::Ready)
    }
}
