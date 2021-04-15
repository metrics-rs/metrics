use atomic_shim::AtomicU64;
use getopts::Options;
use hdrhistogram::Histogram;
use log::{error, info};
use metrics::{gauge, histogram, increment_counter, GaugeValue, Key, Recorder, Unit};
use metrics_util::{Handle, MetricKind, Registry};
use quanta::{Clock, Instant as QuantaInstant};
use std::{
    env,
    ops::Sub,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const LOOP_SAMPLE: u64 = 1000;

pub struct Controller {
    registry: Arc<Registry<Key, Handle>>,
}

impl Controller {
    /// Takes a snapshot of the recorder.
    // Performs the traditional "upkeep" of a recorder i.e. clearing histogram buckets, etc.
    pub fn upkeep(&self) {
        let handles = self.registry.get_handles();

        for ((kind, _), (_, handle)) in handles {
            if matches!(kind, MetricKind::Histogram) {
                handle.read_histogram_with_clear(|_| {});
            }
        }
    }
}

/// A simplistic recorder for benchmarking.
///
/// Simulates typical recorder implementations by utilizing `Registry`, clearing histogram buckets, etc.
pub struct BenchmarkingRecorder {
    registry: Arc<Registry<Key, Handle>>,
}

impl BenchmarkingRecorder {
    /// Creates a new `BenchmarkingRecorder`.
    pub fn new() -> BenchmarkingRecorder {
        BenchmarkingRecorder {
            registry: Arc::new(Registry::new()),
        }
    }

    /// Gets a `Controller` attached to this recorder.
    pub fn controller(&self) -> Controller {
        Controller {
            registry: self.registry.clone(),
        }
    }

    /// Installs this recorder as the global recorder.
    pub fn install(self) -> Result<(), metrics::SetRecorderError> {
        metrics::set_boxed_recorder(Box::new(self))
    }
}

impl Recorder for BenchmarkingRecorder {
    fn register_counter(&self, key: &Key, _unit: Option<Unit>, _description: Option<&'static str>) {
        self.registry
            .op(MetricKind::Counter, key, |_| {}, Handle::counter)
    }

    fn register_gauge(&self, key: &Key, _unit: Option<Unit>, _description: Option<&'static str>) {
        self.registry
            .op(MetricKind::Gauge, key, |_| {}, Handle::gauge)
    }

    fn register_histogram(
        &self,
        key: &Key,
        _unit: Option<Unit>,
        _description: Option<&'static str>,
    ) {
        self.registry
            .op(MetricKind::Histogram, key, |_| {}, Handle::histogram)
    }

    fn increment_counter(&self, key: &Key, value: u64) {
        self.registry.op(
            MetricKind::Counter,
            key,
            |handle| handle.increment_counter(value),
            Handle::counter,
        )
    }

    fn update_gauge(&self, key: &Key, value: GaugeValue) {
        self.registry.op(
            MetricKind::Gauge,
            key,
            |handle| handle.update_gauge(value),
            Handle::gauge,
        )
    }

    fn record_histogram(&self, key: &Key, value: f64) {
        self.registry.op(
            MetricKind::Histogram,
            key,
            |handle| handle.record_histogram(value),
            Handle::histogram,
        )
    }
}

impl Default for BenchmarkingRecorder {
    fn default() -> Self {
        BenchmarkingRecorder::new()
    }
}

struct Generator {
    t0: Option<QuantaInstant>,
    gauge: i64,
    hist: Histogram<u64>,
    done: Arc<AtomicBool>,
    rate_counter: Arc<AtomicU64>,
}

impl Generator {
    fn new(done: Arc<AtomicBool>, rate_counter: Arc<AtomicU64>) -> Generator {
        Generator {
            t0: None,
            gauge: 0,
            hist: Histogram::<u64>::new_with_bounds(1, u64::max_value(), 3).unwrap(),
            done,
            rate_counter,
        }
    }

    fn run(&mut self) {
        let clock = Clock::new();
        let mut counter = 0;
        loop {
            counter += 1;

            self.gauge += 1;

            let t1 = clock.recent();

            if let Some(t0) = self.t0 {
                let start = if counter % LOOP_SAMPLE == 0 {
                    Some(clock.now())
                } else {
                    None
                };

                increment_counter!("ok");
                gauge!("total", self.gauge as f64);
                histogram!("ok", t1.sub(t0));

                if let Some(val) = start {
                    let delta = clock.now() - val;
                    self.hist.saturating_record(delta.as_nanos() as u64);

                    // We also increment our global counter for the sample rate here.
                    self.rate_counter
                        .fetch_add(LOOP_SAMPLE * 3, Ordering::AcqRel);

                    if self.done.load(Ordering::Relaxed) {
                        break;
                    }
                }
            }

            self.t0 = Some(t1);
        }
    }
}

impl Drop for Generator {
    fn drop(&mut self) {
        info!(
            "    sender latency: min: {:8} p50: {:8} p95: {:8} p99: {:8} p999: {:8} max: {:8}",
            nanos_to_readable(self.hist.min()),
            nanos_to_readable(self.hist.value_at_percentile(50.0)),
            nanos_to_readable(self.hist.value_at_percentile(95.0)),
            nanos_to_readable(self.hist.value_at_percentile(99.0)),
            nanos_to_readable(self.hist.value_at_percentile(99.9)),
            nanos_to_readable(self.hist.max())
        );
    }
}

fn print_usage(program: &str, opts: &Options) {
    let brief = format!("Usage: {} [options]", program);
    print!("{}", opts.usage(&brief));
}

pub fn opts() -> Options {
    let mut opts = Options::new();

    opts.optopt(
        "d",
        "duration",
        "number of seconds to run the benchmark",
        "INTEGER",
    );
    opts.optopt("p", "producers", "number of producers", "INTEGER");
    opts.optflag("h", "help", "print this help menu");

    opts
}

fn main() {
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    let program = &args[0];
    let opts = opts();

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(f) => {
            error!("Failed to parse command line args: {}", f);
            return;
        }
    };

    if matches.opt_present("help") {
        print_usage(program, &opts);
        return;
    }

    info!("metrics benchmark");

    // Build our sink and configure the facets.
    let seconds = matches
        .opt_str("duration")
        .unwrap_or_else(|| "60".to_owned())
        .parse()
        .unwrap();
    let producers = matches
        .opt_str("producers")
        .unwrap_or_else(|| "1".to_owned())
        .parse()
        .unwrap();

    info!("duration: {}s", seconds);
    info!("producers: {}", producers);

    let recorder = BenchmarkingRecorder::new();
    let controller = recorder.controller();
    recorder.install().expect("failed to install recorder");

    info!("sink configured");

    // Spin up our sample producers.
    let done = Arc::new(AtomicBool::new(false));
    let rate_counter = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();

    for _ in 0..producers {
        let d = done.clone();
        let r = rate_counter.clone();
        let handle = thread::spawn(move || {
            let mut gen = Generator::new(d, r);
            gen.run();
        });

        handles.push(handle);
    }

    thread::spawn(|| loop {
        thread::sleep(Duration::from_millis(10));
        quanta::set_recent(quanta::Instant::now());
    });

    // Poll the controller to figure out the sample rate.
    let mut total = 0;
    let mut t0 = Instant::now();

    let mut upkeep_hist = Histogram::<u64>::new_with_bounds(1, u64::max_value(), 3).unwrap();
    for _ in 0..seconds {
        let t1 = Instant::now();

        let start = Instant::now();
        controller.upkeep();
        let end = Instant::now();
        upkeep_hist.saturating_record(duration_as_nanos(end - start) as u64);

        let turn_total = rate_counter.load(Ordering::Acquire);
        let turn_delta = turn_total - total;
        total = turn_total;
        let rate = turn_delta as f64 / (duration_as_nanos(t1 - t0) / 1_000_000_000.0);

        info!("sample ingest rate: {:.0} samples/sec", rate);
        t0 = t1;
        thread::sleep(Duration::new(1, 0));
    }

    info!("--------------------------------------------------------------------------------");
    info!(" ingested samples total: {}", total);
    info!(
        "   recorder upkeep: min: {:8} p50: {:8} p95: {:8} p99: {:8} p999: {:8} max: {:8}",
        nanos_to_readable(upkeep_hist.min()),
        nanos_to_readable(upkeep_hist.value_at_percentile(50.0)),
        nanos_to_readable(upkeep_hist.value_at_percentile(95.0)),
        nanos_to_readable(upkeep_hist.value_at_percentile(99.0)),
        nanos_to_readable(upkeep_hist.value_at_percentile(99.9)),
        nanos_to_readable(upkeep_hist.max())
    );

    // Wait for the producers to finish so we can get their stats too.
    done.store(true, Ordering::SeqCst);
    for handle in handles {
        let _ = handle.join();
    }
}

fn duration_as_nanos(d: Duration) -> f64 {
    (d.as_secs() as f64 * 1e9) + d.subsec_nanos() as f64
}

fn nanos_to_readable(t: u64) -> String {
    let f = t as f64;
    if f < 1_000.0 {
        format!("{}ns", f)
    } else if f < 1_000_000.0 {
        format!("{:.0}μs", f / 1_000.0)
    } else if f < 2_000_000_000.0 {
        format!("{:.2}ms", f / 1_000_000.0)
    } else {
        format!("{:.3}s", f / 1_000_000_000.0)
    }
}
