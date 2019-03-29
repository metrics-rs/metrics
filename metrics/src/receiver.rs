use crate::{
    configuration::Configuration,
    control::{ControlFrame, Controller},
    data::{Counter, Gauge, Histogram, Sample, ScopedKey, Snapshot, StringScopedKey},
    scopes::Scopes,
    sink::Sink,
};
use crossbeam_channel::{self, bounded, tick, Select, TryRecvError};
use quanta::Clock;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// Wrapper for all messages that flow over the data channel between sink/receiver.
pub(crate) enum MessageFrame {
    /// A normal data message holding a metric sample.
    Data(Sample),
}

/// Metrics receiver which aggregates and processes samples.
pub struct Receiver {
    config: Configuration,

    // Sample aggregation machinery.
    msg_tx: crossbeam_channel::Sender<MessageFrame>,
    msg_rx: Option<crossbeam_channel::Receiver<MessageFrame>>,
    control_tx: crossbeam_channel::Sender<ControlFrame>,
    control_rx: Option<crossbeam_channel::Receiver<ControlFrame>>,

    // Metric machinery.
    counter: Counter,
    gauge: Gauge,
    thistogram: Histogram,
    vhistogram: Histogram,

    clock: Clock,
    scopes: Arc<Scopes>,
}

impl Receiver {
    pub(crate) fn from_config(config: Configuration) -> Receiver {
        // Create our data, control, and buffer channels.
        let (msg_tx, msg_rx) = bounded(config.capacity);
        let (control_tx, control_rx) = bounded(16);

        let histogram_window = config.histogram_window;
        let histogram_granularity = config.histogram_granularity;

        Receiver {
            config,
            msg_tx,
            msg_rx: Some(msg_rx),
            control_tx,
            control_rx: Some(control_rx),
            counter: Counter::new(),
            gauge: Gauge::new(),
            thistogram: Histogram::new(histogram_window, histogram_granularity),
            vhistogram: Histogram::new(histogram_window, histogram_granularity),
            clock: Clock::new(),
            scopes: Arc::new(Scopes::new()),
        }
    }

    /// Gets a builder to configure a `Receiver` instance with.
    pub fn builder() -> Configuration { Configuration::default() }

    /// Creates a `Sink` bound to this receiver.
    pub fn get_sink(&self) -> Sink {
        Sink::new_with_scope_id(
            self.msg_tx.clone(),
            self.clock.clone(),
            self.scopes.clone(),
            "".to_owned(),
            0,
        )
    }

    /// Creates a `Controller` bound to this receiver.
    pub fn get_controller(&self) -> Controller { Controller::new(self.control_tx.clone()) }

    /// Run the receiver.
    pub fn run(&mut self) {
        let batch_size = self.config.batch_size;
        let mut batch = Vec::with_capacity(batch_size);
        let upkeep_rx = tick(Duration::from_millis(100));
        let control_rx = self.control_rx.take().expect("failed to take control rx");
        let msg_rx = self.msg_rx.take().expect("failed to take msg rx");

        let mut selector = Select::new();
        let _ = selector.recv(&upkeep_rx);
        let _ = selector.recv(&control_rx);
        let _ = selector.recv(&msg_rx);

        loop {
            // Block on having something to do.
            let _ = selector.ready();

            if upkeep_rx.try_recv().is_ok() {
                let now = Instant::now();
                self.thistogram.upkeep(now);
                self.vhistogram.upkeep(now);
            }

            while let Ok(cframe) = control_rx.try_recv() {
                self.process_control_frame(cframe);
            }

            loop {
                match msg_rx.try_recv() {
                    Ok(mframe) => batch.push(mframe),
                    Err(TryRecvError::Empty) => break,
                    Err(e) => eprintln!("error receiving message frame: {}", e),
                }

                if batch.len() == batch_size {
                    break;
                }
            }

            if !batch.is_empty() {
                for mframe in batch.drain(0..) {
                    self.process_msg_frame(mframe);
                }
            }
        }
    }

    /// Gets the string representation of an integer scope.
    ///
    /// Returns `Some(scope)` if found, `None` otherwise.  Scope ID `0` is reserved for the root
    /// scope.
    fn get_string_scope(&self, key: ScopedKey) -> Option<StringScopedKey> {
        let scope_id = key.id();
        if scope_id == 0 {
            return Some(key.into_string_scoped("".to_owned()));
        }

        self.scopes.get(scope_id).map(|scope| key.into_string_scoped(scope))
    }

    /// Gets a snapshot of the current metrics/facets.
    fn get_snapshot(&self) -> Snapshot {
        let mut snapshot = Snapshot::default();
        let cvalues = self.counter.values();
        let gvalues = self.gauge.values();
        let tvalues = self.thistogram.values();
        let vvalues = self.vhistogram.values();

        for (key, value) in cvalues {
            if let Some(actual_key) = self.get_string_scope(key) {
                snapshot.set_count(actual_key, value);
            }
        }

        for (key, value) in gvalues {
            if let Some(actual_key) = self.get_string_scope(key) {
                snapshot.set_gauge(actual_key, value);
            }
        }

        for (key, value) in tvalues {
            if let Some(actual_key) = self.get_string_scope(key) {
                snapshot.set_timing_histogram(actual_key, value);
            }
        }

        for (key, value) in vvalues {
            if let Some(actual_key) = self.get_string_scope(key) {
                snapshot.set_value_histogram(actual_key, value);
            }
        }

        snapshot
    }

    /// Processes a control frame.
    fn process_control_frame(&self, msg: ControlFrame) {
        match msg {
            ControlFrame::Snapshot(tx) => {
                let snapshot = self.get_snapshot();
                let _ = tx.send(snapshot);
            },
            ControlFrame::SnapshotAsync(tx) => {
                let snapshot = self.get_snapshot();
                let _ = tx.send(snapshot);
            },
        }
    }

    /// Processes a message frame.
    fn process_msg_frame(&mut self, msg: MessageFrame) {
        match msg {
            MessageFrame::Data(sample) => {
                match sample {
                    Sample::Count(key, count) => {
                        self.counter.update(key, count);
                    },
                    Sample::Gauge(key, value) => {
                        self.gauge.update(key, value);
                    },
                    Sample::TimingHistogram(key, start, end) => {
                        let delta = end - start;
                        self.counter.update(key.clone(), 1);
                        self.thistogram.update(key, delta);
                    },
                    Sample::ValueHistogram(key, value) => {
                        self.vhistogram.update(key, value);
                    },
                }
            },
        }
    }
}
