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
const QUERY: &[u8] = b"BRI-DISCOVER\0";
const MAX_REPLY: usize = 8192;

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
            self.certificate.len().is_multiple_of(2) && self.certificate.len() <= 32768,
            "Invalid advertised certificate"
        );
        (0..self.certificate.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&self.certificate[i..i + 2], 16)
                    .context("Invalid advertised certificate")
            })
            .collect()
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

/// Answer discovery queries until the returned task is aborted. The live
/// player count comes from the running host.
pub async fn respond(beacon: Beacon, players: Arc<AtomicU32>) -> Result<tokio::task::JoinHandle<()>> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT))
        .await
        .context("Could not open the LAN discovery port")?;
    Ok(tokio::spawn(async move {
        let mut buffer = [0u8; 64];
        loop {
            let Ok((len, from)) = socket.recv_from(&mut buffer).await else {
                continue;
            };
            if &buffer[..len] != QUERY {
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
    }))
}

/// Broadcast (or unicast to `targets`) a query and collect listings until
/// `wait` elapses. Replies are keyed by the host's game address.
pub async fn query(targets: &[SocketAddr], wait: Duration) -> Result<Vec<(SocketAddr, Beacon)>> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
    socket.set_broadcast(true)?;
    for target in targets {
        let _ = socket.send_to(QUERY, target).await;
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
    }
}
