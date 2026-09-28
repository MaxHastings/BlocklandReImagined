//! The host's public address, as seen from the internet, through a public
//! STUN server (RFC 8489 Binding request). Only the address is learned: no
//! account, no relay, nothing kept by the server beyond the request itself.
use anyhow::{Context, Result, bail, ensure};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};
use tokio::net::UdpSocket;

/// Public STUN servers, tried in order. Both are run by large networks for
/// WebRTC and see nothing but the requester's address.
pub const SERVERS: &[&str] = &["stun.cloudflare.com:3478", "stun.l.google.com:19302"];

const MAGIC: u32 = 0x2112_A442;
const BINDING_REQUEST: u16 = 0x0001;
const BINDING_SUCCESS: u16 = 0x0101;
const MAPPED_ADDRESS: u16 = 0x0001;
const XOR_MAPPED_ADDRESS: u16 = 0x0020;

/// Ask each server in turn for this computer's public IPv4 address.
pub async fn public_address(servers: &[&str], wait: Duration) -> Result<SocketAddr> {
    let mut last = None;
    for server in servers {
        let found = async {
            let target = tokio::net::lookup_host(*server)
                .await?
                .find(SocketAddr::is_ipv4)
                .with_context(|| format!("{server} has no IPv4 address"))?;
            query(target, wait).await
        }
        .await;
        match found {
            Ok(address) => return Ok(address),
            Err(error) => last = Some(error),
        }
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!("No STUN server configured")))
        .context("Could not find this computer's public address (no answer from the internet)")
}

/// One Binding request to `server`, sent twice if the first is lost.
pub async fn query(server: SocketAddr, wait: Duration) -> Result<SocketAddr> {
    let bind = if server.is_ipv4() {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    } else {
        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
    };
    let socket = UdpSocket::bind((bind, 0)).await?;
    let mut transaction = [0u8; 12];
    getrandom::fill(&mut transaction).map_err(|e| anyhow::anyhow!("OS randomness failed: {e}"))?;
    let request = request(&transaction);
    let mut buffer = [0u8; 576];
    for _ in 0..2 {
        socket.send_to(&request, server).await?;
        let deadline = tokio::time::Instant::now() + wait / 2;
        while let Ok(Ok((len, from))) =
            tokio::time::timeout_at(deadline, socket.recv_from(&mut buffer)).await
        {
            if from == server
                && let Ok(address) = parse_response(&buffer[..len], &transaction)
            {
                return Ok(address);
            }
        }
    }
    bail!("No answer from STUN server {server}")
}

fn request(transaction: &[u8; 12]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(20);
    packet.extend(BINDING_REQUEST.to_be_bytes());
    packet.extend(0u16.to_be_bytes());
    packet.extend(MAGIC.to_be_bytes());
    packet.extend(transaction);
    packet
}

/// The mapped address in a Binding success response for `transaction`.
fn parse_response(packet: &[u8], transaction: &[u8; 12]) -> Result<SocketAddr> {
    ensure!(packet.len() >= 20, "Short STUN packet");
    let kind = u16::from_be_bytes([packet[0], packet[1]]);
    let length = u16::from_be_bytes([packet[2], packet[3]]) as usize;
    ensure!(
        kind == BINDING_SUCCESS
            && packet[4..8] == MAGIC.to_be_bytes()
            && packet[8..20] == transaction[..]
            && packet.len() == 20 + length,
        "Not an answer to this request"
    );
    let mut plain = None;
    let mut rest = &packet[20..];
    while rest.len() >= 4 {
        let attribute = u16::from_be_bytes([rest[0], rest[1]]);
        let len = u16::from_be_bytes([rest[2], rest[3]]) as usize;
        let value = rest.get(4..4 + len).context("Truncated STUN attribute")?;
        match attribute {
            XOR_MAPPED_ADDRESS => return address(value, Some(transaction)),
            MAPPED_ADDRESS => plain = Some(address(value, None)?),
            _ => {}
        }
        // Attributes are padded to four bytes.
        rest = rest.get(4 + len.div_ceil(4) * 4..).unwrap_or_default();
    }
    plain.context("The STUN answer carried no address")
}

/// A (XOR-)MAPPED-ADDRESS value. `xor` holds the transaction id when the
/// address is obfuscated.
fn address(value: &[u8], xor: Option<&[u8; 12]>) -> Result<SocketAddr> {
    ensure!(value.len() >= 4, "Short STUN address");
    let mask: Vec<u8> = match xor {
        Some(transaction) => MAGIC.to_be_bytes().iter().chain(transaction).copied().collect(),
        None => vec![0; 16],
    };
    let port = u16::from_be_bytes([value[2] ^ mask[0], value[3] ^ mask[1]]);
    let ip = match (value[1], value.len()) {
        (1, 8) => IpAddr::V4(Ipv4Addr::new(
            value[4] ^ mask[0],
            value[5] ^ mask[1],
            value[6] ^ mask[2],
            value[7] ^ mask[3],
        )),
        (2, 20) => {
            let mut octets = [0u8; 16];
            for (i, octet) in octets.iter_mut().enumerate() {
                *octet = value[4 + i] ^ mask[i];
            }
            IpAddr::V6(Ipv6Addr::from(octets))
        }
        _ => bail!("Unknown STUN address family"),
    };
    Ok(SocketAddr::new(ip, port))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A Binding success answering `request` with `mapped`, the way public
    /// servers do (a software attribute, then XOR-MAPPED-ADDRESS).
    pub(crate) fn answer(request: &[u8], mapped: SocketAddr) -> Vec<u8> {
        let SocketAddr::V4(mapped) = mapped else {
            unreachable!()
        };
        let mut attributes = Vec::new();
        attributes.extend(0x8022u16.to_be_bytes());
        attributes.extend(3u16.to_be_bytes());
        attributes.extend(b"bri\0");
        attributes.extend(XOR_MAPPED_ADDRESS.to_be_bytes());
        attributes.extend(8u16.to_be_bytes());
        attributes.extend([0, 1]);
        attributes.extend((mapped.port() ^ (MAGIC >> 16) as u16).to_be_bytes());
        attributes.extend((u32::from(*mapped.ip()) ^ MAGIC).to_be_bytes());
        let mut packet = BINDING_SUCCESS.to_be_bytes().to_vec();
        packet.extend((attributes.len() as u16).to_be_bytes());
        packet.extend(&request[4..20]);
        packet.extend(attributes);
        packet
    }

    #[test]
    fn xor_mapped_addresses_decode_and_strangers_are_ignored() {
        let transaction = [7u8; 12];
        let request = request(&transaction);
        let mapped: SocketAddr = "203.0.113.10:54321".parse().unwrap();
        let packet = answer(&request, mapped);
        assert_eq!(parse_response(&packet, &transaction).unwrap(), mapped);
        assert!(parse_response(&packet, &[8u8; 12]).is_err(), "other transaction");
        assert!(parse_response(&packet[..packet.len() - 2], &transaction).is_err());
        assert!(parse_response(&request, &transaction).is_err(), "a request");
    }

    #[tokio::test]
    async fn a_local_server_reports_the_mapped_address() {
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let at = server.local_addr().unwrap();
        let mapped: SocketAddr = "198.51.100.7:40000".parse().unwrap();
        tokio::spawn(async move {
            let mut buffer = [0u8; 576];
            let (len, from) = server.recv_from(&mut buffer).await.unwrap();
            server.send_to(&answer(&buffer[..len], mapped), from).await.unwrap();
        });
        assert_eq!(query(at, Duration::from_secs(2)).await.unwrap(), mapped);
    }

    #[tokio::test]
    #[ignore = "asks public STUN servers on the internet"]
    async fn public_servers_answer() {
        let address = public_address(SERVERS, Duration::from_secs(3)).await.unwrap();
        println!("public address {address}");
    }
}
