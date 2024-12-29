use std::{net::{SocketAddr, ToSocketAddrs as _}, time::Duration};

#[cfg(target_os = "linux")]
use std::path::PathBuf;

pub mod sync;

#[derive(Clone)]
pub(crate) enum RemoteAddr {
    Udp(Vec<SocketAddr>),

    #[cfg(target_os = "linux")]
    Unixgram(PathBuf),

    #[cfg(target_os = "linux")]
    Unix(PathBuf),
}

impl RemoteAddr {
    /// Returns the transport ID for the remote address.
    ///
    /// This is a simple acronym related to the transport that will be used for the remote address, such as `udp` for
    /// UDP, and so on.
    pub const fn transport_id(&self) -> &'static str {
        match self {
            RemoteAddr::Udp(_) => "udp",
            #[cfg(target_os = "linux")]
            RemoteAddr::Unix(_) => "uds-stream",
            #[cfg(target_os = "linux")]
            RemoteAddr::Unixgram(_) => "uds",
        }
    }
}

impl<'a> TryFrom<&'a str> for RemoteAddr {
    type Error = String;

    fn try_from(addr: &'a str) -> Result<Self, Self::Error> {
        #[cfg(target_os = "linux")]
        if let Some((scheme, path)) = addr.split_once("://") {
            return match scheme {
                "unix" => Ok(RemoteAddr::Unix(PathBuf::from(path))),
                "unixgram" => Ok(RemoteAddr::Unixgram(PathBuf::from(path))),
                _ => Err(format!("invalid scheme '{}' (expected 'unix' or 'unixgram')", scheme)),
            };
        }

        match addr.to_socket_addrs() {
            Ok(addr) => Ok(RemoteAddr::Udp(addr.collect())),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// Forwarder configuration.
#[derive(Clone)]
pub struct ForwarderConfiguration {
    pub remote_addr: RemoteAddr,
    pub max_payload_len: usize,
    pub flush_interval: Duration,
    pub write_timeout: Duration,
}

impl ForwarderConfiguration {
    /// Returns `true` if the remote address requires a length prefix to be sent before each payload.
    pub fn requires_length_prefix(&self) -> bool {
        match self.remote_addr {
            RemoteAddr::Udp(_) => false,
            #[cfg(target_os = "linux")]
            RemoteAddr::Unix(_) => true,
            #[cfg(target_os = "linux")]
            RemoteAddr::Unixgram(_) => true,
        }
    }
}
