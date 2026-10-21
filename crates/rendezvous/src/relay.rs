//! Relay for one host and one player whose routers would not let a direct
//! connection through. Each side gets its own UDP port on the service; every
//! datagram arriving on one port is sent out of the other, unchanged. The
//! game session inside stays end-to-end encrypted and pinned to the host's
//! key, so the relay can neither read nor alter it.
use std::{
    net::{IpAddr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::net::UdpSocket;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Bytes per second forwarded in each direction; more is dropped.
    pub bytes_per_second: u64,
    /// A relay with no traffic for this long is closed.
    pub idle: Duration,
    /// Ports relays may use (inclusive); None lets the OS pick.
    pub ports: Option<(u16, u16)>,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            // A game session is tens of kilobits per second; world downloads
            // burst higher. 1 MiB/s each way leaves room without letting one
            // relay starve the rest.
            bytes_per_second: 1 << 20,
            idle: Duration::from_secs(60),
            ports: None,
        }
    }
}

/// Totals across all relays, for the service's log line.
#[derive(Debug, Default)]
pub struct Stats {
    pub active: AtomicUsize,
    pub forwarded_bytes: AtomicU64,
    pub dropped: AtomicU64,
}

/// Open a relay: returns the (host-side, player-side) ports. Only datagrams
/// from `host_ip` are accepted on the host side and from `joiner_ip` on the
/// player side; the first such datagram fixes the exact address (routers may
/// pick a new port for each destination).
pub async fn open(
    bind: IpAddr,
    host_ip: IpAddr,
    joiner_ip: IpAddr,
    limits: Limits,
    stats: Arc<Stats>,
) -> std::io::Result<(u16, u16)> {
    let host_side = bind_in(bind, limits.ports).await?;
    let joiner_side = bind_in(bind, limits.ports).await?;
    let ports = (
        host_side.local_addr()?.port(),
        joiner_side.local_addr()?.port(),
    );
    stats.active.fetch_add(1, Ordering::Relaxed);
    tokio::spawn(async move {
        run(host_side, joiner_side, host_ip, joiner_ip, limits, &stats).await;
        stats.active.fetch_sub(1, Ordering::Relaxed);
    });
    Ok(ports)
}

async fn bind_in(ip: IpAddr, ports: Option<(u16, u16)>) -> std::io::Result<UdpSocket> {
    let Some((low, high)) = ports else {
        return UdpSocket::bind((ip, 0)).await;
    };
    let span = u32::from(high.saturating_sub(low)) + 1;
    let mut start = [0u8; 4];
    let _ = getrandom::fill(&mut start);
    let start = u32::from_le_bytes(start) % span;
    for i in 0..span {
        let port = low + ((start + i) % span) as u16;
        if let Ok(socket) = UdpSocket::bind((ip, port)).await {
            return Ok(socket);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AddrInUse,
        "Every relay port is in use",
    ))
}

/// Token bucket over one second.
struct Budget {
    left: u64,
    refilled: Instant,
    rate: u64,
}
impl Budget {
    fn new(rate: u64) -> Self {
        Self {
            left: rate,
            refilled: Instant::now(),
            rate,
        }
    }
    fn take(&mut self, bytes: usize) -> bool {
        let now = Instant::now();
        let earned = (now - self.refilled).as_secs_f64() * self.rate as f64;
        if earned >= 1.0 {
            self.left = (self.left + earned as u64).min(self.rate);
            self.refilled = now;
        }
        match self.left.checked_sub(bytes as u64) {
            Some(left) => {
                self.left = left;
                true
            }
            None => false,
        }
    }
}

async fn run(
    host_side: UdpSocket,
    joiner_side: UdpSocket,
    host_ip: IpAddr,
    joiner_ip: IpAddr,
    limits: Limits,
    stats: &Stats,
) {
    let (mut host, mut joiner) = (None::<SocketAddr>, None::<SocketAddr>);
    let (mut to_host, mut to_joiner) = (
        Budget::new(limits.bytes_per_second),
        Budget::new(limits.bytes_per_second),
    );
    let mut from_host = vec![0u8; 2048];
    let mut from_joiner = vec![0u8; 2048];
    loop {
        tokio::select! {
            received = host_side.recv_from(&mut from_host) => {
                let Ok((len, from)) = received else { continue };
                if from.ip() != host_ip || host.is_some_and(|h| h != from) {
                    continue;
                }
                host = Some(from);
                if let Some(joiner) = joiner {
                    if to_joiner.take(len) {
                        let _ = joiner_side.send_to(&from_host[..len], joiner).await;
                        stats.forwarded_bytes.fetch_add(len as u64, Ordering::Relaxed);
                    } else {
                        stats.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            received = joiner_side.recv_from(&mut from_joiner) => {
                let Ok((len, from)) = received else { continue };
                if from.ip() != joiner_ip || joiner.is_some_and(|j| j != from) {
                    continue;
                }
                joiner = Some(from);
                // Before the host has opened its side, the player's packets
                // are dropped; QUIC sends them again.
                if let Some(host) = host {
                    if to_host.take(len) {
                        let _ = host_side.send_to(&from_joiner[..len], host).await;
                        stats.forwarded_bytes.fetch_add(len as u64, Ordering::Relaxed);
                    } else {
                        stats.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            _ = tokio::time::sleep(limits.idle) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn relays_both_ways_only_between_the_two_peers() {
        let local: IpAddr = "127.0.0.1".parse().unwrap();
        let stats = Arc::new(Stats::default());
        let (host_port, joiner_port) = open(local, local, local, Limits::default(), stats.clone())
            .await
            .unwrap();
        let host = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let joiner = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let stranger = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let host_relay = SocketAddr::new(local, host_port);
        let joiner_relay = SocketAddr::new(local, joiner_port);
        // The host opens its side first, then the player talks.
        host.send_to(b"open", host_relay).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        joiner.send_to(b"hello host", joiner_relay).await.unwrap();
        let mut buffer = [0u8; 64];
        let (len, from) = host.recv_from(&mut buffer).await.unwrap();
        assert_eq!((&buffer[..len], from), (&b"hello host"[..], host_relay));
        host.send_to(b"hello player", host_relay).await.unwrap();
        let (len, from) = joiner.recv_from(&mut buffer).await.unwrap();
        assert_eq!((&buffer[..len], from), (&b"hello player"[..], joiner_relay));
        // A third socket cannot hijack either side once it is fixed.
        stranger.send_to(b"sneak", joiner_relay).await.unwrap();
        let quiet =
            tokio::time::timeout(Duration::from_millis(200), host.recv_from(&mut buffer)).await;
        assert!(quiet.is_err(), "the stranger's datagram was forwarded");
        assert_eq!(stats.active.load(Ordering::Relaxed), 1);
    }
    #[test]
    fn the_budget_caps_bytes_per_second() {
        let mut budget = Budget::new(1000);
        assert!(budget.take(600));
        assert!(!budget.take(600));
        assert!(budget.take(400));
    }
    #[tokio::test]
    async fn relay_ports_come_from_the_configured_range() {
        let socket = bind_in("127.0.0.1".parse().unwrap(), Some((42310, 42319)))
            .await
            .unwrap();
        assert!((42310..=42319).contains(&socket.local_addr().unwrap().port()));
    }
}
