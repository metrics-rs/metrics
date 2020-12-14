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
use std::time::{Duration, Instant};

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
    let alpha = 0.00015;
    let max_bins = 32768;
    let sigfigs = 3;
    let quantiles = &[0.1, 0.25, 0.5, 0.9, 0.95, 0.99, 0.999];
    let distribution = generate_distribution(100_000_000, LOW, HIGH);
    let (ddsketch, ddelta, summary, sdelta, histogram, hdelta) =
        build_containers(&distribution, alpha, max_bins, sigfigs);
    let (dsize, ssize, hsize) = estimate_container_sizes(&ddsketch, &summary, &histogram);

    let results = get_quantile_values(distribution, quantiles, ddsketch, summary, histogram);

    println!("----------------- duration comparison -----------------");
    println!(
        "DDSketch configuration: alpha={} max_bins={} min_value=1.0e-9",
        alpha, max_bins
    );
    println!(
        "Summary configuration: alpha={} max_buckets={}, min_value=1.0e-9",
        alpha, max_bins
    );
    println!("HDRHistogram configuration: sigfigs={}", sigfigs);
    println!();
    println!(
        "build time: DDSketch={:.3?}, Summary={:.3?}, HDRHistogram={:.3?}",
        ddelta, sdelta, hdelta
    );
    println!();

    for result in results {
        let quantile = result.quantile;
        let adur = result.actual;
        let ddur = result.ddsketch;
        let sdur = result.summary;
        let hdur = result.hdr;

        let derror = (1.0 - (adur.as_nanos() as f64 / ddur.as_nanos() as f64)) * 100.0;
        let serror = (1.0 - (adur.as_nanos() as f64 / sdur.as_nanos() as f64)) * 100.0;
        let herror = (1.0 - (adur.as_nanos() as f64 / hdur.as_nanos() as f64)) * 100.0;

        let mut deltas = vec![
            ("DDSketch", OrderedFloat::from(derror)),
            ("Summary", OrderedFloat::from(serror)),
            ("HDRHistogram", OrderedFloat::from(herror)),
        ];
        deltas.sort_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap());

        let winner = deltas
            .get(0)
            .map(|(name, _)| name)
            .expect("deltas should have three elements");

        let adurs = format!("{:.3?}", adur);
        let ddurs = format!("{:.3?}", ddur);
        let derrors = format!("{:+.3?}%", derror);
        let sdurs = format!("{:.3?}", sdur);
        let serrors = format!("{:+.3?}%", serror);
        let hdurs = format!("{:.3?}", hdur);
        let herrors = format!("{:+.3?}%", herror);
        println!("quantile={:.3} adur={: >9} ddur={: >9} ({}, {: >7}) sdur={: >9} ({}, {: >7}) hdur={: >9} ({}, {: >7}) [{}]",
            quantile, adurs,
            ddurs, pretty_bytes(dsize as f64), derrors,
            sdurs, pretty_bytes(ssize as f64), serrors,
            hdurs, pretty_bytes(hsize as f64), herrors,
            winner);
    }

    println!("");
    println!("----------------- summary -----------------");
    let squantiles = &[0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];
    let mut rng = thread_rng();
    let dist = Uniform::new(-100.0, 100.0);

    let mut uniform = Vec::new();
    for _ in 0..25_000_000 {
        let value = dist.sample(&mut rng);
        uniform.push(NotNan::new(value).unwrap());
    }

    let mut summary = Summary::new(alpha, max_bins, 1.0e-9);
    for sample in &uniform {
        summary.add(sample.clone().into_inner());
    }

    uniform.sort();
    let mut uniform_array = Array1::from(uniform);

    for quantile in squantiles {
        let sval = summary
            .quantile(*quantile)
            .expect("sketch should not be empty");

        let aval_raw = uniform_array
            .quantile_axis_mut(Axis(0), n64(*quantile), &Linear)
            .expect("quantile should be in range");
        let aval = aval_raw.get(()).expect("quantile value should be present");

        let error = (1.0 - (aval.into_inner() / sval)) * 100.0;

        println!(
            "quantile={:.3} actual={: >14.9} summary={: >14.9} error={:.3}%",
            quantile, aval, sval, error
        );
    }
}
