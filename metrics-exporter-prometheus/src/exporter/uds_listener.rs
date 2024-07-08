use std::net::SocketAddr;

use http_body_util::Full;
use hyper::{
    body::{self, Bytes, Incoming},
    server::conn::http1::Builder as HyperHttpBuilder,
    service::service_fn,
    Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;
use ipnet::IpNet;
use std::path::PathBuf;
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};
use tracing::warn;

use crate::{common::BuildError, ExporterFuture, PrometheusHandle};

struct UnixListeningExporter {
    handle: PrometheusHandle,
}

impl UnixListeningExporter {
    async fn serve(&self, listener: UnixListener) -> Result<(), hyper::Error> {
        loop {
            let stream = match listener.accept().await {
                Ok((stream, _)) => stream,
                Err(e) => {
                    warn!(error = ?e, "Error accepting connection. Ignoring request.");
                    continue;
                }
            };

            self.process_stream(stream).await;
        }
    }

    async fn process_stream(&self, stream: UnixStream) {
        let handle = self.handle.clone();
        let service = service_fn(move |req: Request<body::Incoming>| {
            let handle = handle.clone();
            async move { Ok::<_, hyper::Error>(Self::handle_http_request(&handle, &req)) }
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
        handle: &PrometheusHandle,
        req: &Request<Incoming>,
    ) -> Response<Full<Bytes>> {
        Response::new(match req.uri().path() {
            "/health" => "OK".into(),
            _ => handle.render().into(),
        })
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
pub(crate) fn new_http_uds_listener(
    handle: PrometheusHandle,
    listen_path: PathBuf,
) -> Result<ExporterFuture, BuildError> {
    let listener = UnixListener::bind(listen_path)
        .and_then(|listener| Ok(listener))
        .map_err(|e| BuildError::FailedToCreateHTTPListener(e.to_string()))?;

    let exporter = UnixListeningExporter { handle };

    Ok(Box::pin(async move { exporter.serve(listener).await }))
}

#[cfg(test)]
mod tests {
    use crate::exporter::uds_listener::UnixListeningExporter;

    #[test]
    fn new_forbidden_response_always_succeeds() {
        UnixListeningExporter::new_forbidden_response(); // doesn't panic
    }
}
