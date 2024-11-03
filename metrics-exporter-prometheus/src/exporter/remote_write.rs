use std::time::Duration;

use http_body_util::{BodyExt, Collected, Full};
use hyper::{body::Bytes, Method, Request, Uri};
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use tracing::error;

use crate::PrometheusHandle;

use super::ExporterFuture;

// Creates an ExporterFuture implementing a push gateway.
pub(super) fn new_remote_write(
    endpoint: Uri,
    interval: Duration,
    handle: PrometheusHandle,
) -> ExporterFuture {
    Box::pin(async move {
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .expect("no native root CA certificates found")
            .https_or_http()
            .enable_http1()
            .build();
        let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new())
            .pool_idle_timeout(Duration::from_secs(30))
            .build(https);

        loop {
            // Sleep for `interval` amount of time, and then do a push.
            tokio::time::sleep(interval).await;

            let mut builder = Request::builder();

            let output = handle.render();
            let result = builder.method(Method::PUT).uri(endpoint.clone()).body(Full::from(output));
            let req = match result {
                Ok(req) => req,
                Err(e) => {
                    error!("failed to build push gateway request: {}", e);
                    continue;
                }
            };

            match client.request(req).await {
                Ok(response) => {
                    if !response.status().is_success() {
                        let status = response.status();
                        let status = status.canonical_reason().unwrap_or_else(|| status.as_str());
                        let body = response
                            .into_body()
                            .collect()
                            .await
                            .map(Collected::to_bytes)
                            .map_err(|_| ())
                            .and_then(|b| String::from_utf8(b[..].to_vec()).map_err(|_| ()))
                            .unwrap_or_else(|()| String::from("<failed to read response body>"));
                        error!(
                            message = "unexpected status after pushing metrics to push gateway",
                            status,
                            %body,
                        );
                    }
                }
                Err(e) => error!("error sending request to push gateway: {:?}", e),
            }
        }
    })
}
