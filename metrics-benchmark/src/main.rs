use getopts::Options;
use hdrhistogram::Histogram as HdrHistogram;
use log::{error, info};
use metrics::{
    counter, gauge, histogram, Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder,
    SetRecorderError, SharedString, Unit,
};
use metrics_util::registry::{AtomicStorage, Registry};
use portable_atomic::AtomicU64;
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
    registry: Arc<Registry<Key, AtomicStorage>>,
}

impl Controller {
    /// Takes a snapshot of the recorder.
    /// Performs the traditional "upkeep" of a recorder i.e. clearing histogram buckets, etc.
    pub fn upkeep(&self) {
        let handles = self.registry.get_histogram_handles();

        for (_, histo) in handles {
            histo.clear();
        }
    }
}

/// A simplistic recorder for benchmarking.
///
/// Simulates typical recorder implementations by utilizing `Registry`, clearing histogram buckets, etc.
pub struct BenchmarkingRecorder {
    registry: Arc<Registry<Key, AtomicStorage>>,
}

impl BenchmarkingRecorder {
    /// Creates a new `BenchmarkingRecorder`.
    pub fn new() -> BenchmarkingRecorder {
        BenchmarkingRecorder { registry: Arc::new(Registry::atomic()) }
    }

    /// Gets a `Controller` attached to this recorder.
    pub fn controller(&self) -> Controller {
        Controller { registry: self.registry.clone() }
    }

    /// Installs this recorder as the global recorder.
    pub fn install(self) -> Result<(), SetRecorderError<Self>> {
        metrics::set_global_recorder(self)
    }
}

impl Recorder for BenchmarkingRecorder {
    fn describe_counter(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

    fn describe_gauge(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

    fn describe_histogram(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        self.registry.get_or_create_counter(key, |c| Counter::from_arc(c.clone()))
    }

    fn register_gauge(&self, key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        self.registry.get_or_create_gauge(key, |g| Gauge::from_arc(g.clone()))
    }

    fn register_histogram(&self, key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        self.registry.get_or_create_histogram(key, |h| Histogram::from_arc(h.clone()))
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
    hist: HdrHistogram<u64>,
    done: Arc<AtomicBool>,
    rate_counter: Arc<AtomicU64>,
}

impl Generator {
    fn new(done: Arc<AtomicBool>, rate_counter: Arc<AtomicU64>) -> Generator {
        Generator {
            t0: None,
            gauge: 0,
            hist: HdrHistogram::<u64>::new_with_bounds(1, u64::max_value(), 3).unwrap(),
            done,
            rate_counter,
        }
    }

    fn run_slow(&mut self) {
        let clock = Clock::new();
        let mut loop_counter = 0;

        loop {
            loop_counter += 1;

            self.gauge += 1;

            let t1 = clock.recent();

            if let Some(t0) = self.t0 {
                let start = if loop_counter % LOOP_SAMPLE == 0 { Some(clock.now()) } else { None };

                counter!("ok").increment(1);
                gauge!("total").set(self.gauge as f64);
                histogram!("ok").record(t1.sub(t0));

                if let Some(val) = start {
                    let delta = clock.now() - val;
                    self.hist.saturating_record(delta.as_nanos() as u64);

                    // We also increment our global counter for the sample rate here.
                    self.rate_counter.fetch_add(LOOP_SAMPLE * 3, Ordering::AcqRel);

                    if self.done.load(Ordering::Relaxed) {
                        break;
                    }
                }
            }

            self.t0 = Some(t1);
        }
    }

    fn run_fast(&mut self) {
        let clock = Clock::new();
        let mut loop_counter = 0;

        let counter = counter!("ok");
        let gauge = gauge!("total");
        let histogram = histogram!("ok");

        loop {
            loop_counter += 1;

            self.gauge += 1;

            let t1 = clock.recent();

            if let Some(t0) = self.t0 {
                let start = if loop_counter % LOOP_SAMPLE == 0 { Some(clock.now()) } else { None };

                counter.increment(1);
                gauge.set(self.gauge as f64);
                histogram.record(t1.sub(t0));

                if let Some(val) = start {
                    let delta = clock.now() - val;
                    self.hist.saturating_record(delta.as_nanos() as u64);

                    // We also increment our global counter for the sample rate here.
                    self.rate_counter.fetch_add(LOOP_SAMPLE * 3, Ordering::AcqRel);

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

    opts.optopt("d", "duration", "number of seconds to run the benchmark", "INTEGER");
    opts.optopt(
        "m",
        "mode",
        "whether or run the benchmark in slow or fast mode (static vs dynamic handles)",
        "STRING",
    );
    opts.optopt("p", "producers", "number of producers", "INTEGER");
    opts.optflag("h", "help", "print this help menu");

    opts
}

fn main() {
    pretty_env_logger::init();

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
    let seconds = matches.opt_str("duration").unwrap_or_else(|| "60".to_owned()).parse().unwrap();
    let producers = matches.opt_str("producers").unwrap_or_else(|| "1".to_owned()).parse().unwrap();
    let mode = matches
        .opt_str("mode")
        .map(|s| if s.to_ascii_lowercase() == "fast" { "fast" } else { "slow" })
        .unwrap_or_else(|| "slow")
        .to_owned();

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
        let mode = mode.clone();
        let handle = thread::spawn(move || {
            let mut gen = Generator::new(d, r);
            if mode == "fast" {
                gen.run_fast();
            } else {
                gen.run_slow();
            }
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

    let mut upkeep_hist = HdrHistogram::<u64>::new_with_bounds(1, u64::max_value(), 3).unwrap();
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
