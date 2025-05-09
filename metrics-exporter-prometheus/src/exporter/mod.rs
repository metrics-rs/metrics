#[cfg(all(feature = "http-listener", not(target_arch = "wasm32")))]
use http_listener::HttpListeningError;
#[cfg(any(feature = "http-listener", feature = "push-gateway"))]
use std::future::Future;
#[cfg(all(feature = "http-listener", not(target_arch = "wasm32")))]
use std::net::SocketAddr;
#[cfg(any(feature = "http-listener", feature = "push-gateway"))]
use std::pin::Pin;
#[cfg(feature = "push-gateway")]
use std::time::Duration;

#[cfg(feature = "push-gateway")]
use hyper::Uri;

/// Error types possible from an exporter
#[cfg(any(feature = "http-listener", feature = "push-gateway"))]
#[derive(Debug)]
pub enum ExporterError {
    #[cfg(all(feature = "http-listener", not(target_arch = "wasm32")))]
    HttpListener(HttpListeningError),
    PushGateway(()),
}
#[cfg(all(any(feature = "http-listener", feature = "push-gateway"), not(target_arch = "wasm32")))]
/// Convenience type for Future implementing an exporter.
pub type ExporterFuture = Pin<Box<dyn Future<Output = Result<(), ExporterError>> + Send + 'static>>;
#[cfg(target_arch = "wasm32")]
/// Convenience type for Future implementing an exporter.
pub type ExporterFuture = Pin<Box<dyn Future<Output = Result<(), ExporterError>>>>;

#[cfg(all(feature = "http-listener", not(target_arch = "wasm32")))]
#[derive(Clone, Debug)]
enum ListenDestination {
    Tcp(SocketAddr),
    #[cfg(feature = "uds-listener")]
    Uds(std::path::PathBuf),
}

#[derive(Clone, Debug)]
enum ExporterConfig {
    // Run an HTTP listener on the given `listen_address`.
    #[cfg(all(feature = "http-listener", not(target_arch = "wasm32")))]
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
    #[cfg_attr(
        any(not(feature = "http-listener"), not(feature = "push-gateway"), target_arch = "wasm32"),
        allow(dead_code)
    )]
    fn as_type_str(&self) -> &'static str {
        match self {
            #[cfg(all(feature = "http-listener", not(target_arch = "wasm32")))]
            Self::HttpListener { .. } => "http-listener",
            #[cfg(feature = "push-gateway")]
            Self::PushGateway { .. } => "push-gateway",
            Self::Unconfigured => "unconfigured,",
        }
    }
}

#[cfg(all(feature = "http-listener", not(target_arch = "wasm32")))]
mod http_listener;

#[cfg(feature = "push-gateway")]
mod push_gateway;

pub(crate) mod builder;
