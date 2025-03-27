#[cfg(unix)]
use std::path::PathBuf;
use std::{
    fmt,
    net::{SocketAddr, ToSocketAddrs as _},
    time::Duration,
};

pub mod sync;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RemoteAddr {
    Udp(Vec<SocketAddr>),

    #[cfg(unix)]
    Unixgram(PathBuf),

    #[cfg(unix)]
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
            #[cfg(unix)]
            RemoteAddr::Unix(_) => "uds-stream",
            #[cfg(unix)]
            RemoteAddr::Unixgram(_) => "uds",
        }
    }

    pub(crate) fn default_max_payload_len(&self) -> usize {
        match self {
            RemoteAddr::Udp(_) => 1432,
            #[cfg(unix)]
            RemoteAddr::Unix(_) | RemoteAddr::Unixgram(_) => 8192,
        }
    }
}

impl<'a> TryFrom<&'a str> for RemoteAddr {
    type Error = String;

    fn try_from(addr: &'a str) -> Result<Self, Self::Error> {
        // Try treating the address as a fully-qualified URL, where the scheme is the transport identifier.
        if let Some((scheme, path)) = addr.split_once("://") {
            return match scheme {
                #[cfg(unix)]
                "unix" => Ok(RemoteAddr::Unix(PathBuf::from(path))),
                #[cfg(unix)]
                "unixgram" => Ok(RemoteAddr::Unixgram(PathBuf::from(path))),
                "udp" => match path.to_socket_addrs() {
                    Ok(addr) => Ok(RemoteAddr::Udp(addr.collect())),
                    Err(e) => Err(e.to_string()),
                },
                _ => Err(unknown_scheme_error_str(scheme)),
            };
        }

        // When there's no scheme present, treat the address as a UDP address.
        match addr.to_socket_addrs() {
            Ok(addr) => Ok(RemoteAddr::Udp(addr.collect())),
            Err(e) => Err(e.to_string()),
        }
    }
}

impl fmt::Display for RemoteAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RemoteAddr::Udp(addrs) => {
                if addrs.len() == 1 {
                    write!(f, "udp://{}", addrs[0])
                } else {
                    write!(f, "udp://[")?;

                    for (idx, addr) in addrs.iter().enumerate() {
                        if idx == 0 {
                            write!(f, "{addr}")?;
                        } else {
                            write!(f, ",{addr}")?;
                        }
                    }
                    write!(f, "]")
                }
            }
            #[cfg(unix)]
            RemoteAddr::Unix(path) | RemoteAddr::Unixgram(path) => {
                write!(f, "unixgram://{}", path.display())
            }
        }
    }
}

fn unknown_scheme_error_str(scheme: &str) -> String {
    format!("invalid scheme '{scheme}' (expected 'udp', 'unix', or 'unixgram')")
}

/// Forwarder configuration.
#[derive(Clone)]
pub(crate) struct ForwarderConfiguration {
    pub remote_addr: RemoteAddr,
    pub max_payload_len: usize,
    pub flush_interval: Duration,
    pub write_timeout: Duration,
}

impl ForwarderConfiguration {
    /// Returns `true` if the remote address requires a length prefix to be sent before each payload.
    pub fn is_length_prefixed(&self) -> bool {
        match self.remote_addr {
            RemoteAddr::Udp(_) => false,
            #[cfg(unix)]
            RemoteAddr::Unix(_) => true,
            #[cfg(unix)]
            RemoteAddr::Unixgram(_) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddrV4;

    use super::*;

    #[test]
    fn remote_addr_basic() {
        let addr = RemoteAddr::try_from("127.0.0.1:8125").unwrap();
        let inner_addrs = vec![SocketAddr::V4(SocketAddrV4::new([127, 0, 0, 1].into(), 8125))];
        assert_eq!(addr, RemoteAddr::Udp(inner_addrs));
    }

    #[test]
    fn remote_addr_scheme_udp() {
        let addr = RemoteAddr::try_from("udp://127.0.0.1:8127").unwrap();
        let inner_addrs = vec![SocketAddr::V4(SocketAddrV4::new([127, 0, 0, 1].into(), 8127))];
        assert_eq!(addr, RemoteAddr::Udp(inner_addrs));
    }

    #[test]
    fn remote_addr_scheme_unknown() {
        let addr = RemoteAddr::try_from("spongebob://127.0.0.1:8675");
        assert_eq!(addr, Err(unknown_scheme_error_str("spongebob")));
    }

    #[cfg(unix)]
    mod linux {
        #[test]
        fn remote_addr_scheme_unix() {
            let addr = super::RemoteAddr::try_from("unix:///tmp/dogstatsd.sock").unwrap();
            assert_eq!(addr, super::RemoteAddr::Unix("/tmp/dogstatsd.sock".into()));
        }

        #[test]
        fn remote_addr_scheme_unixgram() {
            let addr = super::RemoteAddr::try_from("unixgram:///tmp/dogstatsd.sock").unwrap();
            assert_eq!(addr, super::RemoteAddr::Unixgram("/tmp/dogstatsd.sock".into()));
        }
    }
}
