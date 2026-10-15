//! OSC output: sends the app's state to a controller (TouchOSC, a Max patch,
//! a lighting desk) so its faders and labels follow what happens on screen.
//! The counterpart of the `osc` input thread.
//!
//! No thread: a connected, non-blocking UDP socket sends a few dozen bytes
//! per message, so the render loop calls [`OscOut::publish`] once per frame.
//! Only values that changed since the last call are sent, so an idle show
//! sends nothing.

use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::mpsc;
use std::time::Duration;

use rosc::{encoder, OscMessage, OscPacket};

/// Re-exported so callers can build values without depending on `rosc`.
pub use rosc::OscType;

/// How long a host name lookup may take before Start gives up.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(3);

/// An address literal needs no lookup. A name is resolved on a helper thread
/// and waited for at most [`RESOLVE_TIMEOUT`], so a mistyped host cannot
/// freeze the interface for as long as the system resolver takes to fail.
fn resolve(host: &str, port: u16) -> Result<SocketAddr, String> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(SocketAddr::new(ip, port));
    }
    let (tx, rx) = mpsc::channel();
    let name = host.to_string();
    std::thread::spawn(move || {
        let _ = tx.send((name.as_str(), port).to_socket_addrs().map(|mut addrs| addrs.next()));
    });
    match rx.recv_timeout(RESOLVE_TIMEOUT) {
        Ok(Ok(Some(addr))) => Ok(addr),
        Ok(Ok(None)) => Err(format!("no address for {host}:{port}")),
        Ok(Err(e)) => Err(format!("cannot resolve {host}:{port}: {e}")),
        Err(_) => Err(format!("resolving {host} took too long")),
    }
}

pub struct OscOut {
    socket: UdpSocket,
    last: HashMap<String, OscType>,
}

impl OscOut {
    /// Resolves `host:port` and opens a UDP socket aimed at it. Nothing is
    /// sent until [`publish`](Self::publish) is called.
    pub fn connect(host: &str, port: u16) -> Result<Self, String> {
        let target = resolve(host, port)?;
        let bind = if target.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" };
        let socket = UdpSocket::bind(bind).map_err(|e| format!("cannot open a UDP socket: {e}"))?;
        socket.connect(target).map_err(|e| format!("cannot reach {target}: {e}"))?;
        socket.set_nonblocking(true).map_err(|e| format!("cannot make the socket non-blocking: {e}"))?;
        Ok(Self { socket, last: HashMap::new() })
    }

    /// Sends every `(address, value)` whose value differs from the one last
    /// sent for that address. Returns how many messages went out. A full
    /// socket buffer or an unreachable target drops the message silently, as
    /// OSC over UDP always may; the value is retried on the next change.
    pub fn publish(&mut self, values: &[(String, OscType)]) -> usize {
        let mut sent = 0;
        for (address, value) in values {
            if self.last.get(address) == Some(value) {
                continue;
            }
            let packet = OscPacket::Message(OscMessage { addr: address.clone(), args: vec![value.clone()] });
            let Ok(bytes) = encoder::encode(&packet) else { continue };
            match self.socket.send(&bytes) {
                Ok(_) => {
                    self.last.insert(address.clone(), value.clone());
                    sent += 1;
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(_) => {}
            }
        }
        sent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn receiver() -> (UdpSocket, u16) {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let port = socket.local_addr().unwrap().port();
        (socket, port)
    }

    fn receive(socket: &UdpSocket) -> Option<OscMessage> {
        let mut buf = [0u8; 1024];
        let n = socket.recv(&mut buf).ok()?;
        match rosc::decoder::decode_udp(&buf[..n]).ok()?.1 {
            OscPacket::Message(m) => Some(m),
            OscPacket::Bundle(_) => None,
        }
    }

    #[test]
    fn a_value_arrives_as_an_osc_message() {
        let (socket, port) = receiver();
        let mut out = OscOut::connect("127.0.0.1", port).unwrap();

        let sent = out.publish(&[("/opendrop/crossfader".to_string(), OscType::Float(0.25))]);

        assert_eq!(sent, 1);
        let message = receive(&socket).expect("a message should arrive");
        assert_eq!(message.addr, "/opendrop/crossfader");
        assert_eq!(message.args, vec![OscType::Float(0.25)]);
    }

    #[test]
    fn unchanged_values_are_not_resent_but_changed_ones_are() {
        let (socket, port) = receiver();
        let mut out = OscOut::connect("127.0.0.1", port).unwrap();
        let fader = |v: f32| vec![("/opendrop/crossfader".to_string(), OscType::Float(v))];

        assert_eq!(out.publish(&fader(0.5)), 1);
        assert_eq!(out.publish(&fader(0.5)), 0);
        assert_eq!(out.publish(&fader(0.6)), 1);
        assert_eq!(receive(&socket).unwrap().args, vec![OscType::Float(0.5)]);
        assert_eq!(receive(&socket).unwrap().args, vec![OscType::Float(0.6)]);
    }

    #[test]
    fn a_fresh_connection_sends_everything_again() {
        let (_socket, port) = receiver();
        let values = vec![("/opendrop/bpm".to_string(), OscType::Float(128.0))];
        let mut first = OscOut::connect("127.0.0.1", port).unwrap();
        assert_eq!(first.publish(&values), 1);
        let mut second = OscOut::connect("127.0.0.1", port).unwrap();
        assert_eq!(second.publish(&values), 1);
    }

    #[test]
    fn an_address_literal_resolves_without_a_lookup_and_localhost_resolves() {
        assert_eq!(resolve("192.168.1.60", 9000).unwrap(), "192.168.1.60:9000".parse().unwrap());
        assert_eq!(resolve("::1", 9000).unwrap().port(), 9000);
        assert!(resolve("localhost", 9000).is_ok());
    }

    #[test]
    fn an_unresolvable_host_is_an_error_not_a_panic() {
        assert!(OscOut::connect("host name with spaces.invalid", 9000).is_err());
    }
}
