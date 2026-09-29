//! LAN host discovery. Hosts answer a UDP broadcast query with their public
//! listing and QUIC certificate, so players can find LAN games and join them
//! without manually importing a certificate (trust on first use on the LAN).
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};
use tokio::net::UdpSocket;

pub const DISCOVERY_PORT: u16 = 28050;
const MAGIC: &[u8] = b"BRI-DISCOVER\0";
/// Queries are padded to a full datagram so a reply is never much larger than
/// its request: the router-forwarded discovery port cannot amplify
/// spoofed-source floods (QUIC's 3x anti-amplification rule).
const QUERY_SIZE: usize = 1200;
const MAX_REPLY: usize = 3 * QUERY_SIZE;
fn query_packet() -> Vec<u8> {
    let mut packet = MAGIC.to_vec();
    packet.resize(QUERY_SIZE, 0);
    packet
}
fn is_query(packet: &[u8]) -> bool {
    packet.len() == QUERY_SIZE && packet.starts_with(MAGIC)
}

/// Public listing. Never contains credentials; the certificate is public.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Beacon {
    pub version: u32,
    pub name: String,
    pub port: u16,
    pub players: u32,
    pub max_players: u32,
    pub map: String,
    pub content_id: String,
    /// Hex-encoded DER certificate the QUIC host presents.
    pub certificate: String,
}
impl Beacon {
    pub fn certificate_der(&self) -> Result<Vec<u8>> {
        ensure!(
            self.certificate.len().is_multiple_of(2) && self.certificate.len() <= MAX_REPLY,
            "Invalid advertised certificate"
        );
        // Untrusted text: decode bytes, never slice a str at arbitrary offsets.
        let nibble = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
        self.certificate
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| Some((nibble(pair[0])? << 4) | nibble(pair[1])?))
            .collect::<Option<_>>()
            .context("Invalid advertised certificate")
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.name.len() <= 128
                && self.map.len() <= 256
                && self.content_id.len() <= 128
                && self.players <= 64
                && self.max_players <= 64
                && !self.name.chars().any(char::is_control),
            "Invalid host listing"
        );
        Ok(())
    }
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Answer discovery queries on `port` (normally [`DISCOVERY_PORT`]; 0 picks a
/// free one) until the returned task is aborted. The live player count comes
/// from the running host. Returns the task and the bound port.
pub async fn respond(
    beacon: Beacon,
    players: Arc<AtomicU32>,
    port: u16,
) -> Result<(tokio::task::JoinHandle<()>, u16)> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))
        .await
        .context("Could not open the LAN discovery port")?;
    let port = socket.local_addr()?.port();
    let task = tokio::spawn(async move {
        let mut buffer = [0u8; QUERY_SIZE + 1];
        loop {
            let (len, from) = match socket.recv_from(&mut buffer).await {
                Ok(received) => received,
                // Windows reports an ICMP port-unreachable from an earlier
                // reply as a receive error; a persistent error must not spin.
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    continue;
                }
            };
            if !is_query(&buffer[..len]) {
                continue;
            }
            let mut reply = beacon.clone();
            reply.players = players.load(Ordering::Relaxed);
            if let Ok(bytes) = serde_json::to_vec(&reply)
                && bytes.len() <= MAX_REPLY
            {
                let _ = socket.send_to(&bytes, from).await;
            }
        }
    });
    Ok((task, port))
}

/// Broadcast (or unicast to `targets`) a query and collect listings until
/// `wait` elapses. Replies are keyed by the host's game address.
pub async fn query(targets: &[SocketAddr], wait: Duration) -> Result<Vec<(SocketAddr, Beacon)>> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
    socket.set_broadcast(true)?;
    let query = query_packet();
    for target in targets {
        let _ = socket.send_to(&query, target).await;
    }
    let mut found: Vec<(SocketAddr, Beacon)> = Vec::new();
    let deadline = tokio::time::Instant::now() + wait;
    let mut buffer = vec![0u8; MAX_REPLY];
    while let Ok(Ok((len, from))) =
        tokio::time::timeout_at(deadline, socket.recv_from(&mut buffer)).await
    {
        let Ok(beacon) = serde_json::from_slice::<Beacon>(&buffer[..len]) else {
            continue;
        };
        if beacon.validate().is_err() || beacon.version != crate::protocol::VERSION {
            continue;
        }
        let address = SocketAddr::new(from.ip(), beacon.port);
        if !found.iter().any(|(a, _)| *a == address) && found.len() < 256 {
            found.push((address, beacon));
        }
    }
    Ok(found)
}

/// The LAN broadcast target.
pub fn broadcast() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::BROADCAST, DISCOVERY_PORT))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn certificate_hex_round_trips() {
        let beacon = Beacon {
            version: crate::protocol::VERSION,
            name: "Host".into(),
            port: 28000,
            players: 1,
            max_players: 8,
            map: "Slate".into(),
            content_id: "id".into(),
            certificate: hex(&[0, 15, 255]),
        };
        assert_eq!(beacon.certificate_der().unwrap(), vec![0, 15, 255]);
        for hostile in ["0g", "é0", "0é", "abc"] {
            let beacon = Beacon {
                certificate: hostile.into(),
                ..beacon.clone()
            };
            assert!(beacon.certificate_der().is_err(), "{hostile:?}");
        }
    }
    #[test]
    fn replies_never_amplify_queries_more_than_threefold() {
        assert!(is_query(&query_packet()));
        assert!(!is_query(MAGIC), "unpadded queries are ignored");
        let beacon = Beacon {
            version: crate::protocol::VERSION,
            name: "h".repeat(128),
            port: 28000,
            players: 64,
            max_players: 64,
            map: "m".repeat(256),
            content_id: "c".repeat(128),
            certificate: hex(&crate::server::HostCertificate::generate().unwrap().der),
        };
        let reply = serde_json::to_vec(&beacon).unwrap();
        assert!(reply.len() <= MAX_REPLY, "{} byte reply", reply.len());
    }
}
