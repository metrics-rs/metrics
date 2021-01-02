use std::env;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

use crossbeam_queue::SegQueue;
use getopts::Options;
use rand::{thread_rng, Rng};
use tracing::{error, info};

const COUNTER_LOOP: usize = 1024;

fn main() {
    tracing_subscriber::fmt()
        .with_ansi(true)
        .with_level(true)
        .init();

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

    info!("segqueue-crusher");

    // Build our sink and configure the facets.
    let duration = matches
        .opt_str("duration")
        .unwrap_or_else(|| "60".to_owned())
        .parse()
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(60));
    let producers = matches
        .opt_str("producers")
        .unwrap_or_else(|| "1".to_owned())
        .parse()
        .unwrap();

    info!("duration: {:?}", duration);
    info!("producers: {}", producers);

    let producer_done = Arc::new(AtomicBool::new(false));
    let producer_counter = Arc::new(AtomicUsize::new(0));
    let producer_total = Arc::new(AtomicUsize::new(0));
    let queue = Arc::new(SegQueue::new());

    let consumer_done = Arc::new(AtomicBool::new(false));
    let consumer_handle = {
        let done = consumer_done.clone();
        let queue = queue.clone();

        thread::spawn(move || run_consumer(done, queue))
    };

    let mut producer_handles = Vec::new();
    for _ in 0..producers {
        let done = producer_done.clone();
        let counter = producer_counter.clone();
        let total = producer_total.clone();
        let queue = queue.clone();

        let handle = thread::spawn(move || run_producer(done, counter, total, queue));
        producer_handles.push(handle)
    }

    // Now let the crusher do its thang.
    thread::sleep(duration);

    producer_done.store(true, Ordering::SeqCst);
    for handle in producer_handles {
        if let Err(e) = handle.join() {
            error!("encountered error for producer: {:?}", e);
        }
    }

    consumer_done.store(true, Ordering::SeqCst);
    match consumer_handle.join() {
        Err(e) => error!("encountered problem for consumer: {:?}", e),
        Ok((ctotal, ccounter)) => {
            let ptotal = producer_total.load(Ordering::SeqCst);
            let pcounter = producer_counter.load(Ordering::SeqCst);

            println!(
                "Producer(s) reported: {} total, with {} values produced",
                ptotal, pcounter
            );
            println!(
                "Consumer reported:    {} total, with {} values consumed",
                ctotal, ccounter
            );
        }
    }
}

fn run_producer(
    done: Arc<AtomicBool>,
    counter: Arc<AtomicUsize>,
    total: Arc<AtomicUsize>,
    queue: Arc<SegQueue<usize>>,
) {
    let mut counter_local = 0;
    let mut total_local = 0;
    let mut rand = thread_rng();

    loop {
        // Every COUNTER_LOOP iterations, do housekeeping.
        if counter_local == COUNTER_LOOP {
            total.fetch_add(total_local, Ordering::Release);
            counter.fetch_add(counter_local, Ordering::Release);

            total_local = 0;
            counter_local = 0;

            if done.load(Ordering::Relaxed) {
                break;
            }
        }

        let value = rand.gen_range(0..1024);
        queue.push(value);

        total_local += value;
        counter_local += 1;
    }

    info!("producer finished");
}

fn run_consumer(done: Arc<AtomicBool>, queue: Arc<SegQueue<usize>>) -> (usize, usize) {
    let mut total = 0;
    let mut counter = 0;
    let mut watch_counter = 0;

    loop {
        // Every COUNTER_LOOP iterations, do housekeeping.
        if watch_counter == COUNTER_LOOP {
            watch_counter = 0;

            if done.load(Ordering::Relaxed) && queue.is_empty() {
                break;
            }
        }

        watch_counter += 1;
        match queue.pop() {
            Some(value) => total += value,
            None => continue,
        }

        counter += 1;
    }

    info!("consumer finished");

    (total, counter)
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
        "number of seconds to run the crusher test",
        "INTEGER",
    );
    opts.optopt("p", "producers", "number of producers", "INTEGER");
    opts.optflag("h", "help", "print this help menu");

    opts
}
