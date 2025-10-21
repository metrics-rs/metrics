#[cfg(feature = "http-listener")]
use http_listener::HttpListeningError;
#[cfg(any(
    feature = "http-listener",
    feature = "push-gateway",
    feature = "push-gateway-no-tls-provider"
))]
use std::future::Future;
#[cfg(feature = "http-listener")]
use std::net::SocketAddr;
#[cfg(any(
    feature = "http-listener",
    feature = "push-gateway",
    feature = "push-gateway-no-tls-provider"
))]
use std::pin::Pin;
#[cfg(any(feature = "push-gateway", feature = "push-gateway-no-tls-provider"))]
use std::time::Duration;

#[cfg(any(feature = "push-gateway", feature = "push-gateway-no-tls-provider"))]
use hyper::Uri;

/// Error types possible from an exporter
#[cfg(any(
    feature = "http-listener",
    feature = "push-gateway",
    feature = "push-gateway-no-tls-provider"
))]
#[derive(Debug)]
pub enum ExporterError {
    #[cfg(feature = "http-listener")]
    HttpListener(HttpListeningError),
    PushGateway(()),
}
/// Convenience type for Future implementing an exporter.
#[cfg(any(
    feature = "http-listener",
    feature = "push-gateway",
    feature = "push-gateway-no-tls-provider"
))]
pub type ExporterFuture = Pin<Box<dyn Future<Output = Result<(), ExporterError>> + Send + 'static>>;

#[cfg(feature = "http-listener")]
#[derive(Clone, Debug)]
enum ListenDestination {
    Tcp(SocketAddr),
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
    #[cfg(any(feature = "push-gateway", feature = "push-gateway-no-tls-provider"))]
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
        not(any(
            feature = "http-listener",
            feature = "push-gateway",
            feature = "push-gateway-no-tls-provider"
        )),
        allow(dead_code)
    )]
    fn as_type_str(&self) -> &'static str {
        match self {
            #[cfg(feature = "http-listener")]
            Self::HttpListener { .. } => "http-listener",
            #[cfg(any(feature = "push-gateway", feature = "push-gateway-no-tls-provider"))]
            Self::PushGateway { .. } => "push-gateway",
            Self::Unconfigured => "unconfigured,",
        }
    }
}

#[cfg(feature = "http-listener")]
mod http_listener;

#[cfg(any(feature = "push-gateway", feature = "push-gateway-no-tls-provider"))]
mod push_gateway;

pub(crate) mod builder;
