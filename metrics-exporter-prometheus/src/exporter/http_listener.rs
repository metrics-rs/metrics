use std::net::SocketAddr;

use http_body_util::Full;
#[cfg(feature = "protobuf")]
use hyper::header::ACCEPT;
use hyper::{
    body::{Bytes, Incoming},
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
#[derive(Debug, thiserror::Error)]
pub enum HttpListeningError {
    /// The HTTP server encountered an error while serving.
    #[error("http server error: {0}")]
    Hyper(hyper::Error),
    /// An I/O error occurred while serving.
    #[error("i/o error: {0}")]
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
        let service =
            service_fn(move |req| Self::handle_http_request(is_allowed, handle.clone(), req));

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
        let service = service_fn(move |req| Self::handle_http_request(true, handle.clone(), req));

        tokio::spawn(async move {
            if let Err(err) =
                HyperHttpBuilder::new().serve_connection(TokioIo::new(stream), service).await
            {
                warn!(error = ?err, "Error serving connection.");
            }
        });
    }

    async fn handle_http_request(
        is_allowed: bool,
        handle: PrometheusHandle,
        req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, hyper::Error> {
        if !is_allowed {
            return Ok(Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Full::<Bytes>::default())
                .unwrap());
        }

        if req.uri().path() == "/health" {
            let mut response = Response::new("OK".into());
            response.headers_mut().append(CONTENT_TYPE, HeaderValue::from_static("text/plain"));
            return Ok(response);
        }

        // Check content negotiation for metrics endpoint
        let response_format = Self::negotiate_content_type(&req);
        let (body, content_type) = match response_format {
            #[cfg(feature = "protobuf")]
            ResponseFormat::Protobuf => {
                let data =
                    tokio::task::spawn_blocking(move || handle.render_protobuf()).await.unwrap();
                (data.into(), crate::protobuf::PROTOBUF_CONTENT_TYPE)
            }
            ResponseFormat::Text => {
                let data = tokio::task::spawn_blocking(move || handle.render()).await.unwrap();
                (data.into(), "text/plain")
            }
        };

        let mut response = Response::new(body);
        response.headers_mut().append(CONTENT_TYPE, HeaderValue::from_static(content_type));
        Ok(response)
    }

    fn negotiate_content_type(req: &Request<Incoming>) -> ResponseFormat {
        #[cfg(feature = "protobuf")]
        {
            let accept_header =
                req.headers().get(ACCEPT).and_then(|value| value.to_str().ok()).unwrap_or("");

            for mime_type in mime::MimeIter::new(accept_header).flatten() {
                if mime_type.type_() == "application"
                    && (mime_type.subtype() == "vnd.google.protobuf"
                        || mime_type.subtype() == "x-protobuf")
                {
                    return ResponseFormat::Protobuf;
                }
            }
        }

        #[cfg(not(feature = "protobuf"))]
        let _ = req;

        ResponseFormat::Text
    }
}

#[derive(Debug, Clone, Copy)]
enum ResponseFormat {
    Text,
    #[cfg(feature = "protobuf")]
    Protobuf,
}

/// Serves the Prometheus scrape endpoint on an already-bound TCP listener.
///
/// This is the same HTTP server that is spawned when configuring [`PrometheusBuilder`] with
/// [`with_http_listener`], exposed as a standalone function so that the listener can be bound
/// after the recorder has been built and installed — for example, once configuration has been
/// read, or when the listener is provided externally (e.g. via socket activation).
///
/// The server responds to GET requests on any request path with the metrics in the Prometheus
/// [exposition format], except for `/health`, which unconditionally responds with `OK`.
///
/// If `allowed_addresses` is `Some`, clients whose IP address is not covered by any of the given
/// networks receive a 403 Forbidden response.
///
/// The returned future serves connections indefinitely and normally never resolves. Dropping it
/// stops accepting new connections; requests already being served run to completion on their own
/// spawned tasks.
///
/// Unlike [`PrometheusBuilder::build`][crate::PrometheusBuilder::build], calling this function
/// does not spawn an upkeep task: the caller is responsible for calling
/// [`PrometheusHandle::run_upkeep`] periodically. See the **Upkeep and maintenance** section in
/// the top-level crate documentation for more information.
///
/// [`PrometheusBuilder`]: crate::PrometheusBuilder
/// [`with_http_listener`]: crate::PrometheusBuilder::with_http_listener
/// [exposition format]: https://prometheus.io/docs/instrumenting/exposition_formats/#text-based-format
///
/// ## Errors
///
/// Only unrecoverable server errors cause the future to resolve with an error; failure to accept
/// or serve an individual connection is logged and does not terminate the server.
pub async fn serve(
    listener: TcpListener,
    handle: PrometheusHandle,
    allowed_addresses: Option<Vec<IpNet>>,
) -> Result<(), HttpListeningError> {
    let exporter = HttpListeningExporter {
        handle,
        allowed_addresses,
        listener_type: ListenerType::Tcp(listener),
    };

    exporter.serve().await
}

/// Serves the Prometheus scrape endpoint on an already-bound Unix domain socket listener.
///
/// This is the Unix domain socket equivalent of [`serve`], matching the behavior of configuring
/// [`PrometheusBuilder`][crate::PrometheusBuilder] with
/// [`with_http_uds_listener`][crate::PrometheusBuilder::with_http_uds_listener]. Refer to
/// [`serve`] for the full details on server behavior and the caller's upkeep responsibilities.
/// No IP allowlisting is performed for Unix domain sockets.
///
/// ## Errors
///
/// Only unrecoverable server errors cause the future to resolve with an error; failure to accept
/// or serve an individual connection is logged and does not terminate the server.
#[cfg(feature = "uds-listener")]
pub async fn serve_uds(
    listener: UnixListener,
    handle: PrometheusHandle,
) -> Result<(), HttpListeningError> {
    let exporter = HttpListeningExporter {
        handle,
        allowed_addresses: None,
        listener_type: ListenerType::Uds(listener),
    };

    exporter.serve().await
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

    Ok(Box::pin(async move {
        serve(listener, handle, allowed_addresses).await.map_err(super::ExporterError::HttpListener)
    }))
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

    Ok(Box::pin(async move {
        serve_uds(listener, handle).await.map_err(super::ExporterError::HttpListener)
    }))
}
