//! End-to-end tests for `Histogram::record_many` through the exporter:
//! it must render exactly like `count` calls to `record(value)`, for every
//! distribution type.

use metrics::histogram;
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder};

#[test]
fn record_many_renders_weighted_classic_histogram() {
    let recorder = PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("inflight_time_us".to_string()),
            &[1.0, 2.0, 4.0, 8.0],
        )
        .unwrap()
        .build_recorder();
    let handle = recorder.handle();

    metrics::with_local_recorder(&recorder, || {
        // e.g. an occupation-time histogram: "spent 1M µs at level 3, then
        // 500k µs at level 5", each recorded as a single O(1) call.
        histogram!("inflight_time_us").record_many(3.0, 1_000_000);
        histogram!("inflight_time_us").record_many(5.0, 500_000);
        histogram!("inflight_time_us").record(3.0);
    });

    let rendered = handle.render();
    for expected in [
        "inflight_time_us_bucket{le=\"4\"} 1000001",
        "inflight_time_us_count 1500001",
        "inflight_time_us_sum 5500003",
    ] {
        assert!(rendered.contains(expected), "missing {:?} in:\n{}", expected, rendered);
    }
}

#[test]
fn record_many_renders_weighted_summary() {
    // Summaries are the default distribution type, so this is the path an
    // unconfigured `record_many` user hits.
    let recorder = PrometheusBuilder::new().build_recorder();
    let handle = recorder.handle();

    metrics::with_local_recorder(&recorder, || {
        histogram!("dwell").record_many(2.0, 750_000);
        histogram!("dwell").record_many(6.0, 250_000);
    });

    let rendered = handle.render();
    assert!(rendered.contains("dwell_count 1000000"), "{}", rendered);
    assert!(rendered.contains("dwell_sum 3000000"), "{}", rendered);

    // 75% of the weight sits at 2.0, so the median must be ~2.0 (within the
    // sketch's relative error).
    let p50_line = rendered
        .lines()
        .find(|l| l.starts_with("dwell{quantile=\"0.5\"}"))
        .expect("missing p50 line");
    let p50: f64 = p50_line.rsplit(' ').next().unwrap().parse().unwrap();
    assert!((p50 - 2.0).abs() < 0.01, "p50 was {}", p50);
}
