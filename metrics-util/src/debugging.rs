//! Debugging utilities.
//!
//! While these utilities are primarily designed to help aid with testing and debugging of exporters
//! and core parts of the `metrics` ecosystem, they can be beneficial for in-process collecting of
//! metrics in some limited cases.

use std::{
    cell::RefCell,
    collections::HashMap,
    fmt::Debug,
    hash::Hash,
    sync::{atomic::Ordering, Arc, Mutex},
};

use crate::{
    kind::MetricKind,
    registry::{AtomicStorage, Registry},
    CompositeKey,
};

use indexmap::IndexMap;
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Recorder, SharedString, Unit};
use ordered_float::OrderedFloat;

thread_local! {
    /// A per-thread version of the debugging registry/state used only when the debugging recorder is configured to run in per-thread mode.
    static PER_THREAD_INNER: RefCell<Option<Inner>> = RefCell::new(None);
}

/// A composite key name that stores both the metric key name and the metric kind.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
struct CompositeKeyName(MetricKind, KeyName);

impl CompositeKeyName {
    /// Creates a new `CompositeKeyName`.
    pub const fn new(kind: MetricKind, key_name: KeyName) -> CompositeKeyName {
        CompositeKeyName(kind, key_name)
    }
}

/// A point-in-time snapshot of all metrics in [`DebuggingRecorder`].
pub struct Snapshot(Vec<(CompositeKey, Option<Unit>, Option<SharedString>, DebugValue)>);

impl Snapshot {
    /// Converts this snapshot to a mapping of metric data, keyed by the metric key itself.
    #[allow(clippy::mutable_key_type)]
    pub fn into_hashmap(
        self,
    ) -> HashMap<CompositeKey, (Option<Unit>, Option<SharedString>, DebugValue)> {
        self.0
            .into_iter()
            .map(|(k, unit, desc, value)| (k, (unit, desc, value)))
            .collect::<HashMap<_, _>>()
    }

    /// Converts this snapshot to a vector of metric data tuples.
    pub fn into_vec(self) -> Vec<(CompositeKey, Option<Unit>, Option<SharedString>, DebugValue)> {
        self.0
    }
}

/// A point-in-time value for a metric exposing raw values.
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum DebugValue {
    /// Counter.
    Counter(u64),
    /// Gauge.
    Gauge(OrderedFloat<f64>),
    /// Histogram.
    Histogram(Vec<OrderedFloat<f64>>),
}

struct Inner {
    registry: Registry<Key, AtomicStorage>,
    seen: Mutex<IndexMap<CompositeKey, ()>>,
    metadata: Mutex<IndexMap<CompositeKeyName, (Option<Unit>, SharedString)>>,
}

impl Inner {
    fn new() -> Self {
        Self {
            registry: Registry::atomic(),
            seen: Mutex::new(IndexMap::new()),
            metadata: Mutex::new(IndexMap::new()),
        }
    }
}

/// Captures point-in-time snapshots of [`DebuggingRecorder`].
pub struct Snapshotter {
    inner: Arc<Inner>,
}

impl Snapshotter {
    /// Takes a snapshot of the recorder.
    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = Vec::new();

        let counters = self.inner.registry.get_counter_handles();
        let gauges = self.inner.registry.get_gauge_handles();
        let histograms = self.inner.registry.get_histogram_handles();

        let seen = self.inner.seen.lock().expect("seen lock poisoned").clone();
        let metadata = self.inner.metadata.lock().expect("metadata lock poisoned").clone();

        for (ck, _) in seen.into_iter() {
            let value = match ck.kind() {
                MetricKind::Counter => {
                    counters.get(ck.key()).map(|c| DebugValue::Counter(c.load(Ordering::SeqCst)))
                }
                MetricKind::Gauge => gauges.get(ck.key()).map(|g| {
                    let value = f64::from_bits(g.load(Ordering::SeqCst));
                    DebugValue::Gauge(value.into())
                }),
                MetricKind::Histogram => histograms.get(ck.key()).map(|h| {
                    let mut values = Vec::new();
                    h.clear_with(|xs| values.extend(xs.iter().map(|f| OrderedFloat::from(*f))));
                    DebugValue::Histogram(values)
                }),
            };

            let ckn = CompositeKeyName::new(ck.kind(), ck.key().name().to_string().into());
            let (unit, desc) = metadata
                .get(&ckn)
                .map(|(u, d)| (u.to_owned(), Some(d.to_owned())))
                .unwrap_or_else(|| (None, None));

            // If there's no value for the key, that means the metric was only ever described, and
            // not registered, so don't emit it.
            if let Some(value) = value {
                snapshot.push((ck, unit, desc, value));
            }
        }

        Snapshot(snapshot)
    }

    /// Takes a snapshot of the recorder for the current thread only.
    ///
    /// If no registry exists for the current thread, `None` is returned. Otherwise, `Some(snapshot)` is returned.
    pub fn current_thread_snapshot() -> Option<Snapshot> {
        PER_THREAD_INNER.with(|maybe_inner| match maybe_inner.borrow().as_ref() {
            None => None,
            Some(inner) => {
                let mut snapshot = Vec::new();

                let counters = inner.registry.get_counter_handles();
                let gauges = inner.registry.get_gauge_handles();
                let histograms = inner.registry.get_histogram_handles();

                let seen = inner.seen.lock().expect("seen lock poisoned").clone();
                let metadata = inner.metadata.lock().expect("metadata lock poisoned").clone();

                for (ck, _) in seen.into_iter() {
                    let value = match ck.kind() {
                        MetricKind::Counter => counters
                            .get(ck.key())
                            .map(|c| DebugValue::Counter(c.load(Ordering::SeqCst))),
                        MetricKind::Gauge => gauges.get(ck.key()).map(|g| {
                            let value = f64::from_bits(g.load(Ordering::SeqCst));
                            DebugValue::Gauge(value.into())
                        }),
                        MetricKind::Histogram => histograms.get(ck.key()).map(|h| {
                            let mut values = Vec::new();
                            h.clear_with(|xs| {
                                values.extend(xs.iter().map(|f| OrderedFloat::from(*f)))
                            });
                            DebugValue::Histogram(values)
                        }),
                    };

                    let ckn = CompositeKeyName::new(ck.kind(), ck.key().name().to_string().into());
                    let (unit, desc) = metadata
                        .get(&ckn)
                        .map(|(u, d)| (u.to_owned(), Some(d.to_owned())))
                        .unwrap_or_else(|| (None, None));

                    // If there's no value for the key, that means the metric was only ever described, and
                    // not registered, so don't emit it.
                    if let Some(value) = value {
                        snapshot.push((ck, unit, desc, value));
                    }
                }

                Some(Snapshot(snapshot))
            }
        })
    }
}

/// A simplistic recorder that can be installed and used for debugging or testing.
///
/// Callers can easily take snapshots of the metrics at any given time and get access
/// to the raw values.
pub struct DebuggingRecorder {
    inner: Arc<Inner>,
    is_per_thread: bool,
}

impl DebuggingRecorder {
    /// Creates a new `DebuggingRecorder`.
    pub fn new() -> DebuggingRecorder {
        DebuggingRecorder { inner: Arc::new(Inner::new()), is_per_thread: false }
    }

    /// Creates a new `DebuggingRecorder` in per-thread mode.
    ///
    /// This sends all metrics to a per-thread registry, such that the snapshotter will only see metrics emitted in the
    /// thread that the `Snapshotter` is used from. Additionally, as keeping a reference to the original `Snapshotter`
    /// around can be tricky, [`Snapshotter::current_thread_snapshot`] can be used to get all of the metrics currently
    /// present in the registry for the calling thread, if any were emitted.
    ///
    /// Please note that this recorder must still be installed, but it can be installed multiple times (if the error
    /// from `install` is ignored) without clearing or removing any of the existing per-thread metrics, so it's safe to
    /// re-create and re-install multiple times in the same test binary if necessary.
    pub fn per_thread() -> DebuggingRecorder {
        DebuggingRecorder { inner: Arc::new(Inner::new()), is_per_thread: true }
    }

    /// Gets a `Snapshotter` attached to this recorder.
    pub fn snapshotter(&self) -> Snapshotter {
        Snapshotter { inner: Arc::clone(&self.inner) }
    }

    fn describe_metric(&self, rkey: CompositeKeyName, unit: Option<Unit>, desc: SharedString) {
        if self.is_per_thread {
            PER_THREAD_INNER.with(|cell| {
                // Create the inner state if it doesn't yet exist.
                //
                // SAFETY: It's safe to use `borrow_mut` here, even though the parent method is `&self`, as this is a
                // per-thread invocation, so no other caller could possibly be holding a referenced, immutable or
                // mutable, at the same time.
                let mut maybe_inner = cell.borrow_mut();
                let inner = maybe_inner.get_or_insert_with(Inner::new);

                let mut metadata = inner.metadata.lock().expect("metadata lock poisoned");
                let (uentry, dentry) = metadata.entry(rkey).or_insert((None, desc.to_owned()));
                if unit.is_some() {
                    *uentry = unit;
                }
                *dentry = desc.to_owned();
            });
        } else {
            let mut metadata = self.inner.metadata.lock().expect("metadata lock poisoned");
            let (uentry, dentry) = metadata.entry(rkey).or_insert((None, desc.to_owned()));
            if unit.is_some() {
                *uentry = unit;
            }
            *dentry = desc;
        }
    }

    fn track_metric(&self, ckey: CompositeKey) {
        if self.is_per_thread {
            PER_THREAD_INNER.with(|cell| {
                // Create the inner state if it doesn't yet exist.
                //
                // SAFETY: It's safe to use `borrow_mut` here, even though the parent method is `&self`, as this is a
                // per-thread invocation, so no other caller could possibly be holding a referenced, immutable or
                // mutable, at the same time.
                let mut maybe_inner = cell.borrow_mut();
                let inner = maybe_inner.get_or_insert_with(Inner::new);

                let mut seen = inner.seen.lock().expect("seen lock poisoned");
                seen.insert(ckey, ());
            });
        } else {
            let mut seen = self.inner.seen.lock().expect("seen lock poisoned");
            seen.insert(ckey, ());
        }
    }

    /// Installs this recorder as the global recorder.
    pub fn install(self) -> Result<(), metrics::SetRecorderError> {
        metrics::set_boxed_recorder(Box::new(self))
    }
}

impl Recorder for DebuggingRecorder {
    fn describe_counter(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        let ckey = CompositeKeyName::new(MetricKind::Counter, key);
        self.describe_metric(ckey, unit, description);
    }

    fn describe_gauge(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        let ckey = CompositeKeyName::new(MetricKind::Gauge, key);
        self.describe_metric(ckey, unit, description);
    }

    fn describe_histogram(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        let ckey = CompositeKeyName::new(MetricKind::Histogram, key);
        self.describe_metric(ckey, unit, description);
    }

    fn register_counter(&self, key: &Key) -> Counter {
        let ckey = CompositeKey::new(MetricKind::Counter, key.clone());
        self.track_metric(ckey);

        if self.is_per_thread {
            PER_THREAD_INNER.with(move |cell| {
                // Create the inner state if it doesn't yet exist.
                //
                // SAFETY: It's safe to use `borrow_mut` here, even though the parent method is `&self`, as this is a
                // per-thread invocation, so no other caller could possibly be holding a referenced, immutable or
                // mutable, at the same time.
                let mut maybe_inner = cell.borrow_mut();
                let inner = maybe_inner.get_or_insert_with(Inner::new);

                inner.registry.get_or_create_counter(key, |c| Counter::from_arc(c.clone()))
            })
        } else {
            self.inner.registry.get_or_create_counter(key, |c| Counter::from_arc(c.clone()))
        }
    }

    fn register_gauge(&self, key: &Key) -> Gauge {
        let ckey = CompositeKey::new(MetricKind::Gauge, key.clone());
        self.track_metric(ckey);

        if self.is_per_thread {
            PER_THREAD_INNER.with(move |cell| {
                // Create the inner state if it doesn't yet exist.
                //
                // SAFETY: It's safe to use `borrow_mut` here, even though the parent method is `&self`, as this is a
                // per-thread invocation, so no other caller could possibly be holding a referenced, immutable or
                // mutable, at the same time.
                let mut maybe_inner = cell.borrow_mut();
                let inner = maybe_inner.get_or_insert_with(Inner::new);

                inner.registry.get_or_create_gauge(key, |g| Gauge::from_arc(g.clone()))
            })
        } else {
            self.inner.registry.get_or_create_gauge(key, |g| Gauge::from_arc(g.clone()))
        }
    }

    fn register_histogram(&self, key: &Key) -> Histogram {
        let ckey = CompositeKey::new(MetricKind::Histogram, key.clone());
        self.track_metric(ckey);

        if self.is_per_thread {
            PER_THREAD_INNER.with(move |cell| {
                // Create the inner state if it doesn't yet exist.
                //
                // SAFETY: It's safe to use `borrow_mut` here, even though the parent method is `&self`, as this is a
                // per-thread invocation, so no other caller could possibly be holding a referenced, immutable or
                // mutable, at the same time.
                let mut maybe_inner = cell.borrow_mut();
                let inner = maybe_inner.get_or_insert_with(Inner::new);

                inner.registry.get_or_create_histogram(key, |h| Histogram::from_arc(h.clone()))
            })
        } else {
            self.inner.registry.get_or_create_histogram(key, |h| Histogram::from_arc(h.clone()))
        }
    }
}

impl Default for DebuggingRecorder {
    fn default() -> Self {
        DebuggingRecorder::new()
    }
}

#[cfg(test)]
mod tests {
    use metrics::counter;

    use crate::{CompositeKey, MetricKind};

    use super::{DebugValue, DebuggingRecorder, Snapshotter};

    #[test]
    fn per_thread() {
        // Create the recorder in per-thread mode, get the snapshotter, and then spawn two threads that record some
        // metrics. Neither thread should see the metrics of the other, and this main thread running the test should see
        // _any_ metrics.
        let recorder = DebuggingRecorder::per_thread();
        let snapshotter = recorder.snapshotter();

        unsafe { metrics::clear_recorder() };
        recorder.install().expect("installing debugging recorder should not fail");

        let t1 = std::thread::spawn(|| {
            counter!("test_counter", 43);

            Snapshotter::current_thread_snapshot()
        });

        let t2 = std::thread::spawn(|| {
            counter!("test_counter", 47);

            Snapshotter::current_thread_snapshot()
        });

        let t1_result =
            t1.join().expect("thread 1 should not fail").expect("thread 1 should have metrics");
        let t2_result =
            t2.join().expect("thread 2 should not fail").expect("thread 2 should have metrics");

        let main_result = snapshotter.snapshot().into_vec();
        assert!(main_result.is_empty());

        assert_eq!(
            t1_result.into_vec(),
            vec![(
                CompositeKey::new(MetricKind::Counter, "test_counter".into()),
                None,
                None,
                DebugValue::Counter(43),
            )]
        );

        assert_eq!(
            t2_result.into_vec(),
            vec![(
                CompositeKey::new(MetricKind::Counter, "test_counter".into()),
                None,
                None,
                DebugValue::Counter(47),
            )]
        );
    }
}
