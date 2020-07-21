#[macro_use]
extern crate log;
extern crate env_logger;
extern crate getopts;
extern crate hdrhistogram;

use atomic_shim::AtomicU64;
use getopts::Options;
use hdrhistogram::Histogram;
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

struct Generator {
    counter: Arc<AtomicU64>,
    clock: Clock,
    hist: Histogram<u64>,
    done: Arc<AtomicBool>,
}

impl Generator {
    fn new(counter: Arc<AtomicU64>, done: Arc<AtomicBool>) -> Generator {
        Generator {
            counter,
            clock: Clock::new(),
            hist: Histogram::<u64>::new_with_bounds(1, u64::max_value(), 3).unwrap(),
            done,
        }
    }

    fn run(&mut self) {
        let mut counter = 0;
        loop {
            if self.done.load(Ordering::Relaxed) {
                break;
            }

            let start = if counter % 100 == 0 {
                self.clock.now()
            } else {
                0
            };

            counter = self.counter.fetch_add(1, Ordering::AcqRel);

            if start != 0 {
                let delta = self.clock.now() - start;
                self.hist.saturating_record(delta);
            }
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

    // Spin up our sample producers.
    let counter = Arc::new(AtomicU64::new(0));
    let done = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();

    for _ in 0..producers {
        let c = counter.clone();
        let d = done.clone();
        let handle = thread::spawn(move || {
            Generator::new(c, d).run();
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
        let turn_total = counter.load(Ordering::Acquire);
        let end = Instant::now();
        snapshot_hist.saturating_record(duration_as_nanos(end - start) as u64);

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
        "snapshot retrieval: min: {:9} p50: {:9} p95: {:9} p99: {:9} p999: {:9} max: {:9}",
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
