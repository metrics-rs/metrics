#[cfg(unix)]
use std::os::unix::net::{UnixDatagram, UnixStream};
use std::{
    io::{self, Write as _},
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket},
    sync::Arc,
    thread::sleep,
    time::Instant,
};
use tracing::{debug, error, trace};

use super::{ForwarderConfiguration, RemoteAddr};
use crate::{
    state::{FlushState, State},
    telemetry::{Telemetry, TelemetryUpdate},
    writer::PayloadWriter,
};

/// Returns the remote addresses to try, in the order they should be attempted.
///
/// IPv4 addresses are tried first: the Datadog Agent will preferentially bind to IPv4 addresses based on its
/// `bind_host` configuration, and only in very specific cases bind _only_ to an IPv6 address. As such, when we observe
/// both IPv4 and IPv6 addresses in our address list, we prefer IPv4 since there's a greater chance of the Agent
/// actually listening on the IPv4 addresses than the IPv6 addresses.
fn remote_addrs_in_preferred_order(addrs: &[SocketAddr]) -> impl Iterator<Item = &SocketAddr> {
    addrs.iter().filter(|addr| addr.is_ipv4()).chain(addrs.iter().filter(|addr| addr.is_ipv6()))
}

/// Returns the local address to bind to in order to connect to the given remote address.
///
/// A socket can only connect within its own address family.
fn udp_bind_addr(addr: &SocketAddr) -> SocketAddr {
    match addr {
        SocketAddr::V4(_) => (Ipv4Addr::UNSPECIFIED, 0).into(),
        SocketAddr::V6(_) => (Ipv6Addr::UNSPECIFIED, 0).into(),
    }
}

/// Connects a UDP socket to the first reachable address in `addrs`.
///
/// Each candidate gets a socket bound in its own address family, so a mixed-family list falls through to the next
/// candidate instead of failing outright. As with `UdpSocket::connect`, the last error is returned if every candidate
/// fails.
fn connect_udp(addrs: &[SocketAddr]) -> io::Result<UdpSocket> {
    let mut last_err = None;

    let addresses = remote_addrs_in_preferred_order(addrs);
    for addr in addresses {
        let bind_addr = udp_bind_addr(addr);
        match UdpSocket::bind(bind_addr).and_then(|socket| {
            socket.connect(addr)?;
            Ok(socket)
        }) {
            Ok(socket) => {
                debug!(remote_addr = %addr, "Connected to remote address.");
                return Ok(socket);
            }
            Err(e) => last_err = Some(e),
        }
    }

    Err(last_err.unwrap_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "could not resolve to any addresses")
    }))
}

enum Client {
    Udp(UdpSocket),

    #[cfg(unix)]
    Unixgram(UnixDatagram),

    #[cfg(unix)]
    Unix(UnixStream),
}

impl Client {
    fn from_forwarder_config(config: &ForwarderConfiguration) -> io::Result<Self> {
        match &config.remote_addr {
            RemoteAddr::Udp(addrs) => {
                let socket = connect_udp(addrs)?;
                socket.set_write_timeout(Some(config.write_timeout))?;
                Ok(Client::Udp(socket))
            }

            #[cfg(unix)]
            RemoteAddr::Unixgram(path) => UnixDatagram::unbound().and_then(|socket| {
                socket.connect(path)?;
                socket.set_write_timeout(Some(config.write_timeout))?;
                Ok(Client::Unixgram(socket))
            }),

            #[cfg(unix)]
            RemoteAddr::Unix(path) => UnixStream::connect(path).and_then(|socket| {
                socket.set_write_timeout(Some(config.write_timeout))?;
                Ok(Client::Unix(socket))
            }),
        }
    }

    fn send(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Client::Udp(socket) => socket.send(buf),

            #[cfg(unix)]
            Client::Unixgram(socket) => socket.send(buf),

            #[cfg(unix)]
            Client::Unix(socket) => match socket.write_all(buf) {
                Ok(()) => Ok(buf.len()),
                Err(e) => Err(e),
            },
        }
    }
}

enum ClientState {
    // Intermediate state during send attempts.
    Inconsistent,

    // Forwarder is currently disconnected.
    Disconnected(ForwarderConfiguration),

    // Forwarder is connected and ready to send metrics.
    Ready(ForwarderConfiguration, Client),
}

impl ClientState {
    fn try_send(&mut self, payload: &[u8]) -> io::Result<usize> {
        loop {
            let old_state = std::mem::replace(self, ClientState::Inconsistent);
            match old_state {
                ClientState::Inconsistent => unreachable!("transitioned _from_ inconsistent state"),
                ClientState::Disconnected(config) => match Client::from_forwarder_config(&config) {
                    Ok(client) => *self = ClientState::Ready(config, client),
                    Err(e) => {
                        *self = ClientState::Disconnected(config);
                        return Err(e);
                    }
                },
                ClientState::Ready(config, mut client) => {
                    let result = client.send(payload);
                    if result.is_ok() {
                        *self = ClientState::Ready(config, client);
                    } else {
                        *self = ClientState::Disconnected(config);
                    }

                    return result;
                }
            }
        }
    }
}

pub(crate) struct Forwarder {
    client_state: ClientState,
    config: ForwarderConfiguration,
    state: Arc<State>,
    telemetry: Option<Telemetry>,
}

impl Forwarder {
    /// Create a new synchronous `Forwarder`.
    pub fn new(config: ForwarderConfiguration, state: Arc<State>) -> Self {
        Forwarder {
            client_state: ClientState::Disconnected(config.clone()),
            config,
            state,
            telemetry: None,
        }
    }

    fn update_telemetry(&mut self, update: &TelemetryUpdate) {
        // If we processed any metrics, update our telemetry.
        //
        // We do it in this lazily-initialized fashion because we need to register our internal telemetry metrics with
        // the global recorder _after_ we've been installed, so that the metrics all flow through the same recorder
        // stack and are affected by any relevant recorder layers, and so on.
        //
        // When we have updates, we know that can only have happened if the recorder was installed and metrics were
        // being processed, so we can safely initialize our telemetry at this point.
        if self.state.telemetry_enabled() && update.had_updates() {
            let telemetry = self
                .telemetry
                .get_or_insert_with(|| Telemetry::new(self.config.remote_addr.transport_id()));
            telemetry.apply_update(update);
        }
    }

    /// Run the forwarder, sending out payloads to the configured remote address at the configured interval.
    pub fn run(mut self) {
        let mut flush_state = FlushState::default();
        let mut writer =
            PayloadWriter::new(self.config.max_payload_len, self.config.is_length_prefixed())
                .with_global_labels(&self.config.global_labels);
        let mut telemetry_update = TelemetryUpdate::default();

        let mut next_flush = Instant::now() + self.config.flush_interval;
        loop {
            // Sleep until our target flush deadline.
            //
            // If the previous flush iteration took longer than the flush interval, we won't sleep at all.
            if let Some(sleep_duration) = next_flush.checked_duration_since(Instant::now()) {
                sleep(sleep_duration);
            }

            // Process our flush, building up all of our payloads.
            //
            // We'll also calculate our next flush time here, so that we can splay out the payloads over the remaining
            // time we have before we should be flushing again.
            next_flush = Instant::now() + self.config.flush_interval;

            telemetry_update.clear();
            self.state.flush(&mut flush_state, &mut writer, &mut telemetry_update);

            // Send out all of the payloads that we've written, but splay them out over the remaining time until our
            // next flush, in order to smooth out the network traffic / processing demands on the Datadog Agent.
            let mut payloads = writer.payloads();
            if u32::try_from(payloads.len()).is_err() {
                error!(num_payloads = payloads.len(), "Too many payloads to send.");
                continue;
            }

            let splay_duration = next_flush.saturating_duration_since(Instant::now());
            debug!(
                ?splay_duration,
                num_payloads = payloads.len(),
                "Splaying payloads over remaining time until next flush."
            );

            let mut payloads_sent = 0;
            let mut payloads_dropped = 0;

            while let Some(payload) = payloads.next_payload() {
                if let Err(e) = self.client_state.try_send(payload) {
                    error!(error = %e, "Failed to send payload.");
                    telemetry_update.track_packet_send_failed(payload.len());
                    payloads_dropped += 1;
                } else {
                    telemetry_update.track_packet_send_succeeded(payload.len());
                    payloads_sent += 1;
                }

                // Figure out how long we should sleep based on the remaining time until the next flush and the number
                // of remaining payloads.
                let next_flush_delta = next_flush.saturating_duration_since(Instant::now());
                let remaining_payloads = u32::try_from(payloads.len()).unwrap();
                let inter_payload_sleep = next_flush_delta / remaining_payloads.saturating_add(1);

                trace!(remaining_payloads, "Sleeping {:?} between payloads.", inter_payload_sleep);
                sleep(inter_payload_sleep);
            }

            debug!(payloads_sent, payloads_dropped, "Finished sending payloads.");

            self.update_telemetry(&telemetry_update);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4(last: u8) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::new(127, 0, 0, last), 8125))
    }

    fn v6(last: u16) -> SocketAddr {
        SocketAddr::from((Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, last), 8125))
    }

    fn ordered(addrs: &[SocketAddr]) -> Vec<SocketAddr> {
        remote_addrs_in_preferred_order(addrs).copied().collect()
    }

    #[test]
    fn udp_bind_addr_matches_remote_address_family() {
        assert_eq!(udp_bind_addr(&v4(1)), SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)));
        assert_eq!(udp_bind_addr(&v6(1)), SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)));
    }

    #[test]
    fn connect_order_prefers_ipv4() {
        assert_eq!(ordered(&[]), vec![]);
        assert_eq!(ordered(&[v4(1)]), vec![v4(1)]);
        assert_eq!(ordered(&[v6(1)]), vec![v6(1)]);
        assert_eq!(ordered(&[v6(1), v4(1)]), vec![v4(1), v6(1)]);
        assert_eq!(ordered(&[v4(1), v6(1)]), vec![v4(1), v6(1)]);
    }

    #[test]
    fn connect_order_preserves_resolver_order_within_family() {
        assert_eq!(ordered(&[v6(1), v4(1), v6(2), v4(2)]), vec![v4(1), v4(2), v6(1), v6(2)]);
    }
}
