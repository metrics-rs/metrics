use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use http_body_util::Full;
use hyper::{
    body::{self, Bytes, Incoming},
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
    inner: Arc<Inner>,
}
#[derive(Clone)]
struct Inner {
    handle: PrometheusHandle,
    allowed_addresses: Option<Vec<IpNet>>,
}

impl HttpListeningExporter {
    async fn serve(&self, listener: std::net::TcpListener) -> Result<(), hyper::Error> {
        // TODO - address panic possibility
        let listener = TcpListener::from_std(listener).unwrap();

        loop {
            let stream = match listener.accept().await {
                Ok((stream, _)) => stream,
                Err(e) => {
                    warn!("Error accepting connection. Ignoring request. Error: {:?}", e);
                    continue;
                }
            };

            let remote_addr = match stream.peer_addr() {
                Ok(remote_address) => remote_address.ip(),
                Err(e) => {
                    warn!("Error obtaining remote address. Ignoring request. Error: {:?}", e);
                    continue;
                }
            };

            self.process_stream(stream, remote_addr).await;
        }
    }

    async fn process_stream(&self, stream: TcpStream, remote_address: IpAddr) {
        let inner = self.inner.clone();
        let service = service_fn(move |req: Request<body::Incoming>| {
            let inner = inner.clone();
            async move { Self::handle_http_request(&inner, remote_address, &req) }
        });

        tokio::task::spawn(async move {
            if let Err(err) =
                HyperHttpBuilder::new().serve_connection(TokioIo::new(stream), service).await
            {
                warn!("Error serving connection.  Error: {:?}", err);
            };
        });
    }

    fn handle_http_request(
        inner: &Arc<Inner>,
        remote_address: IpAddr,
        req: &Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, hyper::Error> {
        let is_allowed = match &inner.allowed_addresses {
            Some(addresses) => addresses.iter().any(|address| address.contains(&remote_address)),
            None => true,
        };

        if is_allowed {
            Ok(Response::new(match req.uri().path() {
                "/health" => "OK".into(),
                _ => inner.handle.render().into(),
            }))
        } else {
            Self::new_forbidden_response()
        }
    }

    fn new_forbidden_response() -> Result<Response<Full<Bytes>>, hyper::Error> {
        // This unwrap should not fail because we don't use any function that
        // can assign an Err to it's inner such as `Builder::header``. A unit test
        // will have to suffice to detect if this fails to hold true.
        Ok(Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Full::<Bytes>::default())
            .unwrap())
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

    let exporter = HttpListeningExporter { inner: Arc::new(Inner { handle, allowed_addresses }) };

    Ok(Box::pin(async move { exporter.serve(listener).await }))
}

#[cfg(test)]
mod tests {
    use crate::exporter::http_listener::HttpListeningExporter;

    #[test]
    fn new_forbidden_response_always_succeeds() {
        assert!(HttpListeningExporter::new_forbidden_response().is_ok()); // and doesn't panic
    }
}
