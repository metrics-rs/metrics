use std::net::SocketAddr;

use http_body_util::Full;
use hyper::{
    body::{self, Bytes, Incoming},
    header::{HeaderValue, CONTENT_TYPE},
    server::conn::http1::Builder as HyperHttpBuilder,
    service::service_fn,
    Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;
use ipnet::IpNet;
use tokio::net::{TcpListener, TcpStream};
use tracing::warn;

use crate::{common::BuildError, ExporterFuture, PrometheusHandle};

struct HttpListeningExporter {
    handle: PrometheusHandle,
    allowed_addresses: Option<Vec<IpNet>>,
}

impl HttpListeningExporter {
    async fn serve(&self, listener: tokio::net::TcpListener) -> Result<(), hyper::Error> {
        loop {
            let stream = match listener.accept().await {
                Ok((stream, _)) => stream,
                Err(e) => {
                    warn!(error = ?e, "Error accepting connection. Ignoring request.");
                    continue;
                }
            };

            let is_allowed = self.allowed_addresses.as_ref().map_or(true, |addrs| {
                stream.peer_addr().map_or_else(
                    |e| {
                        warn!(error = ?e, "Error obtaining remote address.");
                        false
                    },
                    |peer_addr| {
                        let remote_ip = peer_addr.ip();
                        addrs.iter().any(|addr| addr.contains(&remote_ip))
                    },
                )
            });

            self.process_stream(stream, is_allowed).await;
        }
    }

    async fn process_stream(&self, stream: TcpStream, is_allowed: bool) {
        let handle = self.handle.clone();
        let service = service_fn(move |req: Request<body::Incoming>| {
            let handle = handle.clone();
            async move { Ok::<_, hyper::Error>(Self::handle_http_request(is_allowed, &handle, &req)) }
        });

        tokio::spawn(async move {
            if let Err(err) =
                HyperHttpBuilder::new().serve_connection(TokioIo::new(stream), service).await
            {
                warn!(error = ?err, "Error serving connection.");
            };
        });
    }

    fn handle_http_request(
        is_allowed: bool,
        handle: &PrometheusHandle,
        req: &Request<Incoming>,
    ) -> Response<Full<Bytes>> {
        if is_allowed {
            let mut response = Response::new(match req.uri().path() {
                "/health" => "OK".into(),
                _ => handle.render().into(),
            });
            response.headers_mut().append(CONTENT_TYPE, HeaderValue::from_static("text/plain"));
            response
        } else {
            Self::new_forbidden_response()
        }
    }

    fn new_forbidden_response() -> Response<Full<Bytes>> {
        // This unwrap should not fail because we don't use any function that
        // can assign an Err to it's inner such as `Builder::header``. A unit test
        // will have to suffice to detect if this fails to hold true.
        Response::builder().status(StatusCode::FORBIDDEN).body(Full::<Bytes>::default()).unwrap()
    }
}

/// Creates an `ExporterFuture` implementing a http listener that servies prometheus metrics.
///
/// # Errors
/// Will return Err if it cannot bind to the listen address
pub(crate) fn new_http_listener(
    handle: PrometheusHandle,
    listen_address: SocketAddr,
    allowed_addresses: Option<Vec<IpNet>>,
) -> Result<ExporterFuture, BuildError> {
    let listener = std::net::TcpListener::bind(listen_address)
        .and_then(|listener| {
            listener.set_nonblocking(true)?;
            Ok(listener)
        })
        .map_err(|e| BuildError::FailedToCreateHTTPListener(e.to_string()))?;
    let listener = TcpListener::from_std(listener).unwrap();

    let exporter = HttpListeningExporter { handle, allowed_addresses };

    Ok(Box::pin(async move { exporter.serve(listener).await }))
}

#[cfg(test)]
mod tests {
    use crate::exporter::http_listener::HttpListeningExporter;

    #[test]
    fn new_forbidden_response_always_succeeds() {
        HttpListeningExporter::new_forbidden_response(); // doesn't panic
    }
}
