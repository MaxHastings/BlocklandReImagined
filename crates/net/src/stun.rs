//! Asking a public STUN server for this computer's public IPv4 address
//! (RFC 5389 Binding request), for a host whose router did not report it.
//!
//! The game ships no STUN server and none is built in: the player names one
//! (`$Pref::Server::StunServer`, `host:port`) or leaves it empty, which turns
//! this off.
use anyhow::{Context, Result, bail, ensure};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};
use tokio::net::UdpSocket;

const MAGIC: u32 = 0x2112_A442;
const BINDING_REQUEST: u16 = 0x0001;
const BINDING_SUCCESS: u16 = 0x0101;
const MAPPED_ADDRESS: u16 = 0x0001;
const XOR_MAPPED_ADDRESS: u16 = 0x0020;
const TRIES: u32 = 2;
const WAIT: Duration = Duration::from_secs(2);

/// The public IPv4 address `server` (a `host:port`) sees this computer at.
pub async fn public_ip(server: &str) -> Result<Ipv4Addr> {
    let address = tokio::net::lookup_host(server)
        .await
        .with_context(|| format!("Could not look up the STUN server {server}"))?
        .find(SocketAddr::is_ipv4)
        .with_context(|| format!("The STUN server {server} has no IPv4 address"))?;
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.connect(address).await?;
    let mut id = [0u8; 12];
    getrandom::fill(&mut id).map_err(|e| anyhow::anyhow!("OS randomness failed: {e}"))?;
    let request = request(&id);
    let mut buffer = [0u8; 512];
    for _ in 0..TRIES {
        socket.send(&request).await?;
        if let Ok(Ok(length)) = tokio::time::timeout(WAIT, socket.recv(&mut buffer)).await
            && let Ok(ip) = parse(&buffer[..length], &id)
        {
            return Ok(ip);
        }
    }
    bail!("The STUN server {server} did not answer")
}

fn request(id: &[u8; 12]) -> [u8; 20] {
    let mut out = [0u8; 20];
    out[..2].copy_from_slice(&BINDING_REQUEST.to_be_bytes());
    out[4..8].copy_from_slice(&MAGIC.to_be_bytes());
    out[8..].copy_from_slice(id);
    out
}

fn parse(packet: &[u8], id: &[u8; 12]) -> Result<Ipv4Addr> {
    ensure!(packet.len() >= 20, "short STUN reply");
    ensure!(
        u16::from_be_bytes([packet[0], packet[1]]) == BINDING_SUCCESS,
        "not a binding success"
    );
    ensure!(packet[8..20] == id[..], "reply to another request");
    let end = 20 + usize::from(u16::from_be_bytes([packet[2], packet[3]]));
    ensure!(end <= packet.len(), "truncated STUN reply");
    let mut at = 20;
    let mut plain = None;
    while at + 4 <= end {
        let kind = u16::from_be_bytes([packet[at], packet[at + 1]]);
        let length = usize::from(u16::from_be_bytes([packet[at + 2], packet[at + 3]]));
        let value = packet
            .get(at + 4..at + 4 + length)
            .context("truncated attribute")?;
        if value.len() >= 8 && value[1] == 1 {
            let raw = [value[4], value[5], value[6], value[7]];
            match kind {
                XOR_MAPPED_ADDRESS => {
                    return Ok(Ipv4Addr::from(u32::from_be_bytes(raw) ^ MAGIC));
                }
                MAPPED_ADDRESS => plain = Some(Ipv4Addr::from(raw)),
                _ => {}
            }
        }
        at += 4 + length.div_ceil(4) * 4;
    }
    plain
        .map(IpAddr::V4)
        .and_then(|ip| match ip {
            IpAddr::V4(v4) => Some(v4),
            IpAddr::V6(_) => None,
        })
        .context("the STUN reply names no address")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reply(id: &[u8; 12], attribute: u16, ip: Ipv4Addr, xor: bool) -> Vec<u8> {
        let value = if xor {
            u32::from(ip) ^ MAGIC
        } else {
            u32::from(ip)
        };
        let mut out = Vec::new();
        out.extend(BINDING_SUCCESS.to_be_bytes());
        out.extend(12u16.to_be_bytes());
        out.extend(MAGIC.to_be_bytes());
        out.extend(id);
        out.extend(attribute.to_be_bytes());
        out.extend(8u16.to_be_bytes());
        out.extend([0, 1, 0x12, 0x34]);
        out.extend(value.to_be_bytes());
        out
    }
    #[test]
    fn a_binding_reply_names_the_public_address() {
        let id = [7u8; 12];
        let ip = Ipv4Addr::new(203, 0, 113, 10);
        assert_eq!(
            parse(&reply(&id, XOR_MAPPED_ADDRESS, ip, true), &id).unwrap(),
            ip
        );
        assert_eq!(
            parse(&reply(&id, MAPPED_ADDRESS, ip, false), &id).unwrap(),
            ip
        );
        assert!(parse(&reply(&[8; 12], XOR_MAPPED_ADDRESS, ip, true), &id).is_err());
        assert!(parse(&[0; 8], &id).is_err());
    }
    #[tokio::test]
    async fn a_stun_server_on_loopback_is_asked_and_answers() -> Result<()> {
        let server = UdpSocket::bind("127.0.0.1:0").await?;
        let at = server.local_addr()?;
        tokio::spawn(async move {
            let mut buffer = [0u8; 64];
            let Ok((length, from)) = server.recv_from(&mut buffer).await else {
                return;
            };
            assert_eq!(length, 20);
            let id: [u8; 12] = buffer[8..20].try_into().unwrap();
            let _ = server
                .send_to(
                    &reply(
                        &id,
                        XOR_MAPPED_ADDRESS,
                        Ipv4Addr::new(198, 51, 100, 9),
                        true,
                    ),
                    from,
                )
                .await;
        });
        assert_eq!(
            public_ip(&at.to_string()).await?,
            Ipv4Addr::new(198, 51, 100, 9)
        );
        Ok(())
    }
}
