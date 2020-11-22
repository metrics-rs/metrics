//! Records metrics in the Prometheus exposition format.
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg), deny(broken_intra_doc_links))]
mod common;
pub use self::common::{Matcher, MetricType};

mod builder;
pub use self::builder::PrometheusBuilder;

mod recorder;
pub use self::recorder::{PrometheusHandle, PrometheusRecorder};

#[cfg(test)]
mod tests {
    use super::{Matcher, MetricType, PrometheusBuilder};
    use metrics::{Key, KeyData, Recorder};
    use quanta::Clock;
    use std::time::Duration;

    #[test]
    fn test_creation() {
        let recorder = PrometheusBuilder::new().build();
        assert!(recorder.is_ok());
    }

    #[test]
    fn test_render() {
        let recorder = PrometheusBuilder::new()
            .build()
            .expect("failed to create PrometheusRecorder");

        let key = Key::from(KeyData::from_name("basic_counter"));
        recorder.increment_counter(key, 42);

        let handle = recorder.handle();
        let rendered = handle.render();
        let expected_counter = "# TYPE basic_counter counter\nbasic_counter 42\n\n";

        assert_eq!(rendered, expected_counter);

        let key = Key::from(KeyData::from_name("basic_gauge"));
        recorder.update_gauge(key, -3.14);
        let rendered = handle.render();
        let expected_gauge = format!(
            "{}# TYPE basic_gauge gauge\nbasic_gauge -3.14\n\n",
            expected_counter
        );

        assert_eq!(rendered, expected_gauge);

        let key = Key::from(KeyData::from_name("basic_histogram"));
        recorder.record_histogram(key, 12);
        let rendered = handle.render();

        let histogram_data = concat!(
            "# TYPE basic_histogram summary\n",
            "basic_histogram{quantile=\"0\"} 12\n",
            "basic_histogram{quantile=\"0.5\"} 12\n",
            "basic_histogram{quantile=\"0.9\"} 12\n",
            "basic_histogram{quantile=\"0.95\"} 12\n",
            "basic_histogram{quantile=\"0.99\"} 12\n",
            "basic_histogram{quantile=\"0.999\"} 12\n",
            "basic_histogram{quantile=\"1\"} 12\n",
            "basic_histogram_sum 12\n",
            "basic_histogram_count 1\n",
            "\n"
        );
        let expected_histogram = format!("{}{}", expected_gauge, histogram_data);

        assert_eq!(rendered, expected_histogram);
    }

    #[test]
    fn test_buckets() {
        const DEFAULT_VALUES: [u64; 3] = [10, 100, 1000];
        const PREFIX_VALUES: [u64; 3] = [15, 105, 1005];
        const SUFFIX_VALUES: [u64; 3] = [20, 110, 1010];
        const FULL_VALUES: [u64; 3] = [25, 115, 1015];

        let recorder = PrometheusBuilder::new()
            .set_buckets_for_metric(
                Matcher::Full("metrics_testing_foo".to_owned()),
                &FULL_VALUES[..],
            )
            .set_buckets_for_metric(
                Matcher::Prefix("metrics_testing".to_owned()),
                &PREFIX_VALUES[..],
            )
            .set_buckets_for_metric(Matcher::Suffix("foo".to_owned()), &SUFFIX_VALUES[..])
            .set_buckets(&DEFAULT_VALUES[..])
            .build()
            .expect("failed to create PrometheusRecorder");

        let full_key = Key::from(KeyData::from_name("metrics_testing_foo"));
        recorder.record_histogram(full_key, FULL_VALUES[0]);

        let prefix_key = Key::from(KeyData::from_name("metrics_testing_bar"));
        recorder.record_histogram(prefix_key, PREFIX_VALUES[1]);

        let suffix_key = Key::from(KeyData::from_name("metrics_testin_foo"));
        recorder.record_histogram(suffix_key, SUFFIX_VALUES[2]);

        let default_key = Key::from(KeyData::from_name("metrics_wee"));
        recorder.record_histogram(default_key, DEFAULT_VALUES[2] + 1);

        let full_data = concat!(
            "# TYPE metrics_testing_foo histogram\n",
            "metrics_testing_foo_bucket{le=\"25\"} 1\n",
            "metrics_testing_foo_bucket{le=\"115\"} 1\n",
            "metrics_testing_foo_bucket{le=\"1015\"} 1\n",
            "metrics_testing_foo_bucket{le=\"+Inf\"} 1\n",
            "metrics_testing_foo_sum 25\n",
            "metrics_testing_foo_count 1\n",
        );

        let prefix_data = concat!(
            "# TYPE metrics_testing_bar histogram\n",
            "metrics_testing_bar_bucket{le=\"15\"} 0\n",
            "metrics_testing_bar_bucket{le=\"105\"} 1\n",
            "metrics_testing_bar_bucket{le=\"1005\"} 1\n",
            "metrics_testing_bar_bucket{le=\"+Inf\"} 1\n",
            "metrics_testing_bar_sum 105\n",
            "metrics_testing_bar_count 1\n",
        );

        let suffix_data = concat!(
            "# TYPE metrics_testin_foo histogram\n",
            "metrics_testin_foo_bucket{le=\"20\"} 0\n",
            "metrics_testin_foo_bucket{le=\"110\"} 0\n",
            "metrics_testin_foo_bucket{le=\"1010\"} 1\n",
            "metrics_testin_foo_bucket{le=\"+Inf\"} 1\n",
            "metrics_testin_foo_sum 1010\n",
            "metrics_testin_foo_count 1\n",
        );

        let default_data = concat!(
            "# TYPE metrics_wee histogram\n",
            "metrics_wee_bucket{le=\"10\"} 0\n",
            "metrics_wee_bucket{le=\"100\"} 0\n",
            "metrics_wee_bucket{le=\"1000\"} 0\n",
            "metrics_wee_bucket{le=\"+Inf\"} 1\n",
            "metrics_wee_sum 1001\n",
            "metrics_wee_count 1\n",
        );

        let handle = recorder.handle();
        let rendered = handle.render();

        assert!(rendered.contains(full_data));
        assert!(rendered.contains(prefix_data));
        assert!(rendered.contains(suffix_data));
        assert!(rendered.contains(default_data));
    }

    #[test]
    fn test_idle_timeout() {
        let (clock, mock) = Clock::mock();

        let recorder = PrometheusBuilder::new()
            .idle_timeout(Some(Duration::from_secs(10)), MetricType::COUNTER)
            .build_with_clock(clock)
            .expect("failed to create PrometheusRecorder");

        let key = Key::from(KeyData::from_name("basic_counter"));
        recorder.increment_counter(key, 42);

        let key = Key::from(KeyData::from_name("basic_gauge"));
        recorder.update_gauge(key, -3.14);

        let handle = recorder.handle();
        let rendered = handle.render();
        let expected = concat!(
            "# TYPE basic_counter counter\n",
            "basic_counter 42\n\n",
            "# TYPE basic_gauge gauge\n",
            "basic_gauge -3.14\n\n",
        );

        assert_eq!(rendered, expected);

        mock.increment(Duration::from_secs(9));
        let rendered = handle.render();
        assert_eq!(rendered, expected);

        mock.increment(Duration::from_secs(2));
        let rendered = handle.render();

        let expected = "# TYPE basic_gauge gauge\nbasic_gauge -3.14\n\n";
        assert_eq!(rendered, expected);
    }
}
