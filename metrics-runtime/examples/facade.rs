#[macro_use]
extern crate log;
extern crate env_logger;
extern crate getopts;
extern crate hdrhistogram;
extern crate metrics_core;
extern crate metrics_runtime;
extern crate tokio;

#[macro_use]
extern crate metrics;

use atomic_shim::AtomicU64;
use getopts::Options;
use hdrhistogram::Histogram;
use metrics_runtime::{exporters::HttpExporter, observers::JsonBuilder, Receiver};
use quanta::Clock;
use std::{
    env,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const LOOP_SAMPLE: u64 = 1000;

struct Generator {
    t0: Option<u64>,
    gauge: i64,
    hist: Histogram<u64>,
    done: Arc<AtomicBool>,
    rate_counter: Arc<AtomicU64>,
    clock: Clock,
}

impl Generator {
    fn new(done: Arc<AtomicBool>, rate_counter: Arc<AtomicU64>, clock: Clock) -> Generator {
        Generator {
            t0: None,
            gauge: 0,
            hist: Histogram::<u64>::new_with_bounds(1, u64::max_value(), 3).unwrap(),
            done,
            rate_counter,
            clock,
        }
    }

    fn run(&mut self) {
        let mut counter = 0;
        loop {
            counter += 1;

            if self.done.load(Ordering::Relaxed) {
                break;
            }

            self.gauge += 1;

            let t1 = self.clock.now();

            if let Some(t0) = self.t0 {
                let start = if counter % LOOP_SAMPLE == 0 {
                    self.clock.now()
                } else {
                    0
                };

                counter!("ok.gotem", 1);
                timing!("ok.gotem", t0, t1);
                gauge!("total", self.gauge);

                if start != 0 {
                    let delta = self.clock.now() - start;
                    self.hist.saturating_record(delta);

                    // We also increment our global counter for the sample rate here.
                    self.rate_counter
                        .fetch_add(LOOP_SAMPLE * 3, Ordering::AcqRel);
                }
            }

            self.t0 = Some(t1);
        }
    }
}

impl Drop for Generator {
    fn drop(&mut self) {
        info!(
            "    sender latency: min: {:9} p50: {:9} p95: {:9} p99: {:9} p999: {:9} max: {:9}",
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

#[tokio::main]
async fn main() {
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

    let receiver = Receiver::builder()
        .histogram(Duration::from_secs(5), Duration::from_millis(100))
        .build()
        .expect("failed to build receiver");

    let controller = receiver.controller();

    let addr = "0.0.0.0:23432"
        .parse()
        .expect("failed to parse http listen address");
    let builder = JsonBuilder::new().set_pretty_json(true);
    let exporter = HttpExporter::new(controller.clone(), builder, addr);
    tokio::spawn(exporter.async_run());

    receiver.install();
    info!("receiver configured");

    // Spin up our sample producers.
    let done = Arc::new(AtomicBool::new(false));
    let rate_counter = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();
    let clock = Clock::new();

    for _ in 0..producers {
        let d = done.clone();
        let r = rate_counter.clone();
        let c = clock.clone();
        let handle = thread::spawn(move || {
            Generator::new(d, r, c).run();
        });

        handles.push(handle);
    }

    // Poll the controller to figure out the sample rate.
    let mut total = 0;
    let mut t0 = Instant::now();

    let mut snapshot_hist = Histogram::<u64>::new_with_bounds(1, u64::max_value(), 3).unwrap();
    for _ in 0..seconds {
        let t1 = Instant::now();

        let start = Instant::now();
        let _snapshot = controller.snapshot();
        let end = Instant::now();
        snapshot_hist.saturating_record(duration_as_nanos(end - start) as u64);

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
        "snapshot end-to-end: min: {:9} p50: {:9} p95: {:9} p99: {:9} p999: {:9} max: {:9}",
        nanos_to_readable(snapshot_hist.min()),
        nanos_to_readable(snapshot_hist.value_at_percentile(50.0)),
        nanos_to_readable(snapshot_hist.value_at_percentile(95.0)),
        nanos_to_readable(snapshot_hist.value_at_percentile(99.0)),
        nanos_to_readable(snapshot_hist.value_at_percentile(99.9)),
        nanos_to_readable(snapshot_hist.max())
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
