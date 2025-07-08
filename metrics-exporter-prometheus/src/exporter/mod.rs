#[cfg(feature = "http-listener")]
use http_listener::HttpListeningError;
#[cfg(any(feature = "http-listener", feature = "push-gateway"))]
use std::future::Future;
#[cfg(any(feature = "http-listener", feature = "push-gateway"))]
use std::pin::Pin;
#[cfg(feature = "push-gateway")]
use std::time::Duration;
#[cfg(feature = "http-listener")]
use std::{net::SocketAddr, sync::Arc};
#[cfg(feature = "http-listener")]
use tokio::net::TcpListener;

#[cfg(feature = "push-gateway")]
use hyper::Uri;

/// Error types possible from an exporter
#[cfg(any(feature = "http-listener", feature = "push-gateway"))]
#[derive(Debug)]
pub enum ExporterError {
    #[cfg(feature = "http-listener")]
    HttpListener(HttpListeningError),
    PushGateway(()),
}
/// Convenience type for Future implementing an exporter.
#[cfg(any(feature = "http-listener", feature = "push-gateway"))]
pub type ExporterFuture = Pin<Box<dyn Future<Output = Result<(), ExporterError>> + Send + 'static>>;

#[cfg(feature = "http-listener")]
#[derive(Clone, Debug)]
enum ListenDestination {
    Tcp(SocketAddr),
    ExistingListener(Arc<TcpListener>),
    #[cfg(feature = "uds-listener")]
    Uds(std::path::PathBuf),
}

#[derive(Clone, Debug)]
enum ExporterConfig {
    // Run an HTTP listener on the given `listen_address`.
    #[cfg(feature = "http-listener")]
    HttpListener { destination: ListenDestination },

    // Run a push gateway task sending to the given `endpoint` after `interval` time has elapsed,
    // infinitely.
    #[cfg(feature = "push-gateway")]
    PushGateway {
        endpoint: Uri,
        interval: Duration,
        username: Option<String>,
        password: Option<String>,
        use_http_post_method: bool,
    },

    #[allow(dead_code)]
    Unconfigured,
}

impl ExporterConfig {
    #[cfg_attr(not(any(feature = "http-listener", feature = "push-gateway")), allow(dead_code))]
    fn as_type_str(&self) -> &'static str {
        match self {
            #[cfg(feature = "http-listener")]
            Self::HttpListener { .. } => "http-listener",
            #[cfg(feature = "push-gateway")]
            Self::PushGateway { .. } => "push-gateway",
            Self::Unconfigured => "unconfigured,",
        }
    }
}

#[cfg(feature = "http-listener")]
mod http_listener;

#[cfg(feature = "push-gateway")]
mod push_gateway;

pub(crate) mod builder;
