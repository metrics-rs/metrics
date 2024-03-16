#[cfg(any(feature = "http-listener", feature = "push-gateway"))]
use std::future::Future;
#[cfg(feature = "http-listener")]
use std::net::SocketAddr;
#[cfg(any(feature = "http-listener", feature = "push-gateway"))]
use std::pin::Pin;
#[cfg(feature = "push-gateway")]
use std::time::Duration;

#[cfg(feature = "push-gateway")]
use hyper::Uri;

/// Convenience type for Future implementing an exporter.
#[cfg(any(feature = "http-listener", feature = "push-gateway"))]
pub type ExporterFuture = Pin<Box<dyn Future<Output = Result<(), hyper::Error>> + Send + 'static>>;

#[derive(Clone)]
enum ExporterConfig {
    // Run an HTTP listener on the given `listen_address`.
    #[cfg(feature = "http-listener")]
    HttpListener { listen_address: SocketAddr },

    // Run a push gateway task sending to the given `endpoint` after `interval` time has elapsed,
    // infinitely.
    #[cfg(feature = "push-gateway")]
    PushGateway {
        endpoint: Uri,
        interval: Duration,
        username: Option<String>,
        password: Option<String>,
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
