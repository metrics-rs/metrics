use metrics_util::Summary;
use ndarray::{Array1, Axis};
use ndarray_stats::{interpolate::Linear, QuantileExt};
use noisy_float::types::n64;
use ordered_float::NotNan;
use rand::{distributions::Distribution, thread_rng};
use rand_distr::Uniform;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::io::BufReader;
use std::process::Command;

fn main() -> io::Result<()> {
    // Generates a consistent set of inputs, and uses them to feed to a reference DDSketch
    // implementation so we can get the quantiles produced for our comparison.
    println!("generating uniform distribution...");
    let mut rng = thread_rng();
    let dist = Uniform::new(-25.0, 75.0);

    let mut summary = Summary::new(0.0001, 32768, 1.0e-9);
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
        .arg("32768")
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
