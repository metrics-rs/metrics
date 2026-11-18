#![cfg(any(
    feature = "http-listener",
    feature = "push-gateway",
    feature = "push-gateway-no-tls-provider"
))]

use std::future::Future;

use metrics_exporter_prometheus::{ExporterError, ExporterFuture};

fn assert_exporter_future<F: Future<Output = Result<(), ExporterError>>>() {}

#[test]
fn exporter_error_is_public_and_matches_exporter_future_output() {
    assert_exporter_future::<ExporterFuture>();

    let error = ExporterError::PushGateway(());
    match error {
        ExporterError::PushGateway(()) => {}
        #[cfg(feature = "http-listener")]
        ExporterError::HttpListener(_) => {}
    }
}
