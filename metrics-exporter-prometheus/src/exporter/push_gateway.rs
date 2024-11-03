use std::time::Duration;

use http_body_util::{BodyExt, Collected, Full};
use hyper::body::Bytes;
use hyper::{header::HeaderValue, Method, Request, Uri};
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use tracing::error;

use super::ExporterFuture;
use crate::PrometheusHandle;

// Creates an ExporterFuture implementing a push gateway.
pub(super) fn new_push_gateway(
    endpoint: Uri,
    interval: Duration,
    username: Option<String>,
    password: Option<String>,
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

        let auth = username.as_ref().map(|name| basic_auth(name, password.as_deref()));

        loop {
            // Sleep for `interval` amount of time, and then do a push.
            tokio::time::sleep(interval).await;

            let mut builder = Request::builder();
            if let Some(auth) = &auth {
                builder = builder.header("authorization", auth.clone());
            }

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

#[cfg(feature = "push-gateway")]
fn basic_auth(username: &str, password: Option<&str>) -> HeaderValue {
    use base64::prelude::BASE64_STANDARD;
    use base64::write::EncoderWriter;
    use std::io::Write;

    let mut buf = b"Basic ".to_vec();
    {
        let mut encoder = EncoderWriter::new(&mut buf, &BASE64_STANDARD);
        write!(encoder, "{username}:").expect("should not fail to encode username");
        if let Some(password) = password {
            write!(encoder, "{password}").expect("should not fail to encode password");
        }
    }
    let mut header = HeaderValue::from_bytes(&buf).expect("base64 is always valid HeaderValue");
    header.set_sensitive(true);
    header
}

#[cfg(test)]
mod tests {
    use super::basic_auth;

    #[test]
    #[allow(clippy::similar_names)] // reader vs header, sheesh clippy
    pub fn test_basic_auth() {
        use base64::prelude::BASE64_STANDARD;
        use base64::read::DecoderReader;
        use std::io::Read;

        const BASIC: &str = "Basic ";

        // username only
        let username = "metrics";
        let header = basic_auth(username, None);

        let reader = &header.as_ref()[BASIC.len()..];
        let mut decoder = DecoderReader::new(reader, &BASE64_STANDARD);
        let mut result = Vec::new();
        decoder.read_to_end(&mut result).unwrap();
        assert_eq!(b"metrics:", &result[..]);
        assert!(header.is_sensitive());

        // username/password
        let password = "123!_@ABC";
        let header = basic_auth(username, Some(password));

        let reader = &header.as_ref()[BASIC.len()..];
        let mut decoder = DecoderReader::new(reader, &BASE64_STANDARD);
        let mut result = Vec::new();
        decoder.read_to_end(&mut result).unwrap();
        assert_eq!(b"metrics:123!_@ABC", &result[..]);
        assert!(header.is_sensitive());
    }
}
