use hdrhistogram::Histogram;
use metrics_util::Summary;
use ndarray::{Array1, Axis};
use ndarray_stats::{interpolate::Linear, QuantileExt};
use noisy_float::types::n64;
use ordered_float::{NotNan, OrderedFloat};
use pretty_bytes::converter::convert as pretty_bytes;
use rand::{distributions::Distribution, thread_rng};
use rand_distr::{Gamma, Uniform};
use sketches_ddsketch::{Config, DDSketch};
use std::env;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::io::BufReader;
use std::process::Command;
use std::time::{Duration, Instant};
use textplots::{Chart, Plot, Shape};

const LOW: f64 = 0.003; // 3ms
const HIGH: f64 = 2.015; // 2.015s

struct QuantileResult {
    pub quantile: f64,
    pub actual: Duration,
    pub ddsketch: Duration,
    pub summary: Duration,
    pub hdr: Duration,
}

fn generate_distribution(samples: usize, low: f64, high: f64) -> Vec<Duration> {
    let mut rng = thread_rng();
    let dist = Gamma::new(1.3, 0.1).expect("failed to generate gamma distribution");
    let delta = high - low;

    let mut durations = Vec::new();
    for _ in 0..samples {
        let value = low + (dist.sample(&mut rng) * delta);
        let dur = Duration::from_secs_f64(value);
        durations.push(dur);
    }

    durations
}

fn build_containers(
    distribution: &Vec<Duration>,
    alpha: f64,
    max_bins: u32,
    sigfigs: u8,
) -> (
    DDSketch,
    Duration,
    Summary,
    Duration,
    Histogram<u64>,
    Duration,
) {
    let config = Config::new(alpha, max_bins, 1.0e-9);

    let mut ddsketch = DDSketch::new(config);
    let mut summary = Summary::new(alpha, max_bins, 1.0e-9);
    let mut histogram = Histogram::<u64>::new(sigfigs).expect("failed to create histogram");

    let dstart = Instant::now();
    for sample in distribution {
        ddsketch.add(sample.as_secs_f64());
    }
    let ddelta = dstart.elapsed();

    let sstart = Instant::now();
    for sample in distribution {
        summary.add(sample.as_secs_f64());
    }
    let sdelta = sstart.elapsed();

    let hstart = Instant::now();
    for sample in distribution {
        histogram
            .record(sample.as_nanos() as u64)
            .expect("failed to record value");
    }
    let hdelta = hstart.elapsed();

    (ddsketch, ddelta, summary, sdelta, histogram, hdelta)
}

fn get_quantile_values(
    distribution: Vec<Duration>,
    quantiles: &[f64],
    sketch: DDSketch,
    summary: Summary,
    histogram: Histogram<u64>,
) -> Vec<QuantileResult> {
    let mut dist_ints = distribution
        .into_iter()
        .map(|d| d.as_nanos() as u64)
        .collect::<Vec<_>>();
    dist_ints.sort();
    let mut dist_array = Array1::from(dist_ints);

    let mut results = Vec::new();
    for quantile in quantiles {
        let dval = sketch
            .quantile(*quantile)
            .expect("quantile should be in range")
            .expect("sketch should not be empty");
        let sval = summary
            .quantile(*quantile)
            .expect("quantile should be in range + sketch should not be empty");
        let hval = histogram.value_at_quantile(*quantile);
        let aval_raw = dist_array
            .quantile_axis_mut(Axis(0), n64(*quantile), &Linear)
            .expect("quantile should be in range");
        let aval = aval_raw.get(()).expect("quantile value should be present");

        let ddur = Duration::from_secs_f64(dval);
        let sdur = Duration::from_secs_f64(sval);
        let hdur = Duration::from_nanos(hval);
        let adur = Duration::from_nanos(*aval);

        results.push(QuantileResult {
            quantile: *quantile,
            actual: adur,
            ddsketch: ddur,
            summary: sdur,
            hdr: hdur,
        })
    }

    results
}

fn estimate_container_sizes(
    sketch: &DDSketch,
    summary: &Summary,
    hdr: &Histogram<u64>,
) -> (usize, usize, usize) {
    let sketch_size = sketch.length() * 8;

    let summary_size = summary.size();

    let hdr_max_sur = 2 * 10usize.pow(hdr.sigfig() as u32);
    let hdr_subbucket = hdr_max_sur.next_power_of_two();
    let hdr_tracked_stride = hdr.max() as f64 / hdr_subbucket as f64;
    let hdr_size = 512 + 4 * (hdr_tracked_stride.log2().ceil() + 2.0) as usize * hdr_subbucket;

    (sketch_size, summary_size, hdr_size)
}

fn main() {
    let operation = env::args()
        .skip(1)
        .next()
        .expect("operation must be specified")
        .to_string();
    match operation.as_str() {
        "run_comparison" => run_comparison().expect("failed to run comparison"),
        "run_summary" => run_summary(),
        op => panic!("unknown operation '{}'", op),
    }
}

fn run_comparison() -> io::Result<()> {
    // Generates a consistent set of inputs, and uses them to feed to a reference DDSketch
    // implementation so we can get the quantiles produced for our comparison.
    println!("generating uniform distribution...");
    let mut rng = thread_rng();
    let dist = Uniform::new(-25.0, 75.0);

    let mut summary = Summary::new(0.0001, 65536, 1.0e-9);
    let mut uniform = Vec::new();
    for _ in 0..1_000_000 {
        let value = dist.sample(&mut rng);
        uniform.push(NotNan::new(value).unwrap());
        summary.add(value);
    }

    println!("writing out distribution to disk...");
    let mut of = File::create("input.csv")?;
    for value in &uniform {
        write!(of, "{}\n", value)?;
    }
    of.flush()?;
    of.sync_all()?;

    println!("generating quantiles from reference DDSketch implementation...");

    // Now run the reference implementation.
    let mut handle = Command::new("../ddsketch-reference-generator/main.py")
        .arg("input.csv")
        .arg("output.csv")
        .arg("0.0001")
        .arg("65536")
        .spawn()?;
    let status = handle.wait()?;
    if !status.success() {
        println!("failed to run DDSketch reference generator");
    }

    println!("parsing reference DDSketch results...");

    // We should now have a file sitting at output.txt with the results.
    let mut dresults = Vec::new();
    let df = File::open("output.csv")?;
    let dreader = BufReader::new(df);
    let mut lines = dreader.lines();
    while let Some(line) = lines.next() {
        let line = line?;
        let mut qv = line.splitn(2, ',');

        let q = qv
            .next()
            .expect("expected quantile")
            .parse::<f64>()
            .expect("failed to parse quantile string as f64");
        let v = qv
            .next()
            .expect("expected value")
            .parse::<f64>()
            .expect("failed to parse value string as f64");

        dresults.push((q, v));
    }

    println!("got {} quantile/value pairs from DDSketch", dresults.len());

    uniform.sort();
    let mut uniform_array = Array1::from(uniform);

    let aresults = (0..1000)
        .map(|x| x as f64 / 1000.0)
        .map(|q| {
            let aval_raw = uniform_array
                .quantile_axis_mut(Axis(0), n64(q), &Linear)
                .expect("quantile should be in range");
            aval_raw
                .get(())
                .expect("quantile value should be present")
                .into_inner()
        })
        .collect::<Vec<_>>();

    let sresults = (0..1000)
        .map(|x| x as f64 / 1000.0)
        .map(|q| (q, summary.quantile(q).unwrap()))
        .collect::<Vec<_>>();

    let dlines = dresults
        .iter()
        .zip(aresults.iter())
        .map(|((q, dval), aval)| (*q as f32, ((1.0 - (aval / dval)) * 100.0) as f32))
        .collect::<Vec<_>>();

    let slines = sresults
        .iter()
        .zip(aresults.iter())
        .map(|((q, sval), aval)| (*q as f32, ((1.0 - (aval / sval)) * 100.0) as f32))
        .collect::<Vec<_>>();

    println!("writing out error rates to disk...");
    let mut of = File::create("results.csv")?;
    for (q, value) in &slines {
        write!(of, "{},{},\"Summary\"\n", q, value)?;
    }
    for (q, value) in &dlines {
        write!(of, "{},{},\"DDSketch\"\n", q, value)?;
    }
    of.flush()?;
    of.sync_all()?;

    Ok(())
}

fn run_summary() {
    let alpha = 0.0001;
    let max_bins = 32768;

    let mut rng = thread_rng();
    let dist = Uniform::new(0.0, 100.0);

    let mut uniform = Vec::new();
    let mut summary = Summary::new(alpha, max_bins, 1.0e-9);
    for _ in 0..1_000_000 {
        let value = dist.sample(&mut rng);
        uniform.push(NotNan::new(value).unwrap());
        summary.add(value);
    }

    let cnt = summary.detailed_counts();
    println!(
        "summary stats: count={} (z={}/n={}/p={}) min={} max={}",
        summary.count(),
        cnt.0,
        cnt.1,
        cnt.2,
        summary.min(),
        summary.max()
    );

    uniform.sort();
    let mut uniform_array = Array1::from(uniform);

    let slines = (0..1000)
        .map(|x| x as f64 / 1000.0)
        .map(|q| {
            let sval = summary.quantile(q).expect("sketch should not be empty");

            let aval_raw = uniform_array
                .quantile_axis_mut(Axis(0), n64(q), &Linear)
                .expect("quantile should be in range");
            let aval = aval_raw.get(()).expect("quantile value should be present");

            let error = ((1.0 - (aval.into_inner() / sval)) * 100.0) as f32;

            (q as f32, error)
        })
        .collect::<Vec<_>>();

    println!("----------------- summary -----------------");
    Chart::new(450, 100, 0.0, 1.0)
        .lineplot(&Shape::Lines(&slines))
        .nice();
}
