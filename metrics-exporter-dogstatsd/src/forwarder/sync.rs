use std::{
    io::{self, Write as _},
    net::{Ipv4Addr, UdpSocket},
    sync::Arc,
    thread::sleep,
    time::Instant,
};

#[cfg(target_os = "linux")]
use std::os::unix::net::{UnixDatagram, UnixStream};

use tracing::{debug, error, trace};

use crate::{
    state::State,
    telemetry::{Telemetry, TelemetryUpdate},
    writer::PayloadWriter,
};

use super::{ForwarderConfiguration, RemoteAddr};

enum Client {
    Udp(UdpSocket),

    #[cfg(target_os = "linux")]
    Unixgram(UnixDatagram),

    #[cfg(target_os = "linux")]
    Unix(UnixStream),
}

impl Client {
    fn from_forwarder_config(config: &ForwarderConfiguration) -> io::Result<Self> {
        match &config.remote_addr {
            RemoteAddr::Udp(addrs) => {
                UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).and_then(|socket| {
                    socket.connect(&addrs[..])?;
                    socket.set_write_timeout(Some(config.write_timeout))?;
                    Ok(Client::Udp(socket))
                })
            }

            #[cfg(target_os = "linux")]
            RemoteAddr::Unixgram(path) => UnixDatagram::bind(path)
                .and_then(|socket| {
                    socket.set_write_timeout(Some(config.write_timeout))?;
                    Ok(Client::Unixgram(socket))
                }),

            #[cfg(target_os = "linux")]
            RemoteAddr::Unix(path) => UnixStream::connect(path)
                .and_then(|socket| {
                    socket.set_write_timeout(Some(config.write_timeout))?;
                    Ok(Client::Unix(socket))
                }),
        }
    }

    fn send(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Client::Udp(socket) => socket.send(buf),

            #[cfg(target_os = "linux")]
            Client::Unixgram(socket) => socket.send(buf),

            #[cfg(target_os = "linux")]
            Client::Unix(socket) => match socket.write_all(buf) {
                Ok(_) => Ok(buf.len()),
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
                ClientState::Disconnected(config) => {
                    let client = Client::from_forwarder_config(&config)?;
                    *self = ClientState::Ready(config, client)
                }
                ClientState::Ready(config, mut client) => {
                    let result = client.send(payload);
                    if result.is_ok() {
                        *self = ClientState::Ready(config, client);
                    } else {
                        *self = ClientState::Disconnected(config);
                    }

                    return result;
                }
            };
        }
    }
}

pub struct Forwarder {
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
            let telemetry = self.telemetry.get_or_insert_with(|| Telemetry::new(self.config.remote_addr.transport_id()));
            telemetry.apply_update(update);
        }
    }

    /// Run the forwarder, sending out payloads to the configured remote address at the configured interval.
    pub fn run(mut self) {
        let mut writer =
            PayloadWriter::new(self.config.max_payload_len, self.config.requires_length_prefix());

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
            self.state.flush(&mut writer, &mut telemetry_update);

            // Send out all of the payloads that we've written, but splay them out over the remaining time until our
            // next flush, in order to smooth out the network traffic / processing demands on the Datadog Agent.
            let mut payloads = writer.payloads();

            let splay_duration = next_flush.saturating_duration_since(Instant::now());
            debug!(
                ?splay_duration,
                num_payloads = payloads.len(),
                "Splaying payloads over remaining time until next flush."
            );

            let mut payloads_sent = 0;
            let mut payloads_dropped = 0;

            while let Some(payload) = payloads.next_payload() {
                if let Err(e) = self.client_state.try_send(&payload) {
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
                let remaining_payloads = payloads.len();
                let inter_payload_sleep = next_flush_delta / (remaining_payloads as u32 + 1);

                trace!(remaining_payloads, "Sleeping {:?} between payloads.", inter_payload_sleep);
                sleep(inter_payload_sleep);
            }

            debug!(payloads_sent, payloads_dropped, "Finished sending payloads.");

            self.update_telemetry(&telemetry_update);
        }
    }
}
