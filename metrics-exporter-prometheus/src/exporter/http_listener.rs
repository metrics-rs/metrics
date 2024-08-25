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
#[cfg(feature = "uds-listener")]
use std::path::PathBuf;
use tokio::net::{TcpListener, TcpStream};
#[cfg(feature = "uds-listener")]
use tokio::net::{UnixListener, UnixStream};
use tracing::warn;

use crate::{common::BuildError, ExporterFuture, PrometheusHandle};

struct HttpListeningExporter {
    handle: PrometheusHandle,
    allowed_addresses: Option<Vec<IpNet>>,
    listener_type: ListenerType,
}

enum ListenerType {
    Tcp(TcpListener),
    #[cfg(feature = "uds-listener")]
    Uds(UnixListener),
}

/// Error type for HTTP listening.
pub enum HttpListeningError {
    Hyper(hyper::Error),
    Io(std::io::Error),
}

impl HttpListeningExporter {
    pub async fn serve(&self) -> Result<(), HttpListeningError> {
        match &self.listener_type {
            ListenerType::Tcp(listener) => {
                self.serve_tcp(listener).await.map_err(HttpListeningError::Hyper)
            }
            #[cfg(feature = "uds-listener")]
            ListenerType::Uds(listener) => {
                self.serve_uds(listener).await.map_err(HttpListeningError::Io)
            }
        }
    }

    async fn serve_tcp(&self, listener: &TcpListener) -> Result<(), hyper::Error> {
        loop {
            let stream = match listener.accept().await {
                Ok((stream, _)) => stream,
                Err(e) => {
                    warn!(error = ?e, "Error accepting connection. Ignoring request.");
                    continue;
                }
            };
            self.process_tcp_stream(stream);
        }
    }

    fn process_tcp_stream(&self, stream: TcpStream) {
        let is_allowed = self.check_tcp_allowed(&stream);
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
            }
        });
    }

    fn check_tcp_allowed(&self, stream: &TcpStream) -> bool {
        let Some(addrs) = &self.allowed_addresses else {
            // No allowed addresses specified, so everything is allowed
            return true;
        };
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
    }

    #[cfg(feature = "uds-listener")]
    async fn serve_uds(&self, listener: &UnixListener) -> Result<(), std::io::Error> {
        loop {
            let stream = match listener.accept().await {
                Ok((stream, _)) => stream,
                Err(e) => {
                    warn!(error = ?e, "Error accepting connection. Ignoring request.");
                    continue;
                }
            };
            self.process_uds_stream(stream);
        }
    }

    #[cfg(feature = "uds-listener")]
    fn process_uds_stream(&self, stream: UnixStream) {
        let handle = self.handle.clone();
        let service = service_fn(move |req: Request<body::Incoming>| {
            let handle = handle.clone();
            async move { Ok::<_, hyper::Error>(Self::handle_http_request(true, &handle, &req)) }
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
            // This unwrap should not fail because we don't use any function that
            // can assign an Err to it's inner such as `Builder::header``. A unit test
            // will have to suffice to detect if this fails to hold true.
            Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Full::<Bytes>::default())
                .unwrap()
        }
    }
}

/// Creates an `ExporterFuture` implementing a http listener that serves prometheus metrics.
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

    let exporter = HttpListeningExporter {
        handle,
        allowed_addresses,
        listener_type: ListenerType::Tcp(listener),
    };

    Ok(Box::pin(async move { exporter.serve().await.map_err(super::ExporterError::HttpListener) }))
}

/// Creates an `ExporterFuture` implementing a http listener that serves prometheus metrics.
/// Binds a Unix Domain socket on the specified `listen_path`
///
/// # Errors
/// Will return Err if it cannot bind to the listen path
#[cfg(feature = "uds-listener")]
pub(crate) fn new_http_uds_listener(
    handle: PrometheusHandle,
    listen_path: PathBuf,
) -> Result<ExporterFuture, BuildError> {
    if listen_path.exists() {
        std::fs::remove_file(&listen_path)
            .map_err(|e| BuildError::FailedToCreateHTTPListener(e.to_string()))?;
    }
    let listener = UnixListener::bind(listen_path)
        .map_err(|e| BuildError::FailedToCreateHTTPListener(e.to_string()))?;
    let exporter = HttpListeningExporter {
        handle,
        allowed_addresses: None,
        listener_type: ListenerType::Uds(listener),
    };

    Ok(Box::pin(async move { exporter.serve().await.map_err(super::ExporterError::HttpListener) }))
}
