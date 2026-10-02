//! An impaired network link for soak and stress tests: a UDP relay between
//! QUIC clients and a host that adds latency, jitter, loss and duplication in
//! both directions. Clients connect to [`ImpairedLink::address`] instead of
//! the host; everything else is the real transport. Jitter larger than the
//! gap between packets reorders them, as the Internet does.
use anyhow::Result;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{net::UdpSocket, task::JoinHandle};

/// What the link does to every datagram, independently in each direction.
#[derive(Debug, Clone, Copy)]
pub struct Impairment {
    /// One-way delay before jitter.
    pub latency: Duration,
    /// Extra one-way delay, uniform in `0..=jitter`.
    pub jitter: Duration,
    /// Probability a datagram is dropped.
    pub loss: f64,
    /// Probability a delivered datagram is delivered twice.
    pub duplicate: f64,
}
impl Impairment {
    /// A clean link.
    pub const NONE: Self = Self {
        latency: Duration::ZERO,
        jitter: Duration::ZERO,
        loss: 0.0,
        duplicate: 0.0,
    };
    /// A poor but playable home connection.
    pub const BAD_WIFI: Self = Self {
        latency: Duration::from_millis(60),
        jitter: Duration::from_millis(40),
        loss: 0.05,
        duplicate: 0.01,
    };
}

/// Datagrams the link has seen, for assertions.
#[derive(Debug, Default)]
pub struct LinkStats {
    pub forwarded: AtomicU64,
    pub dropped: AtomicU64,
    pub duplicated: AtomicU64,
}

pub struct ImpairedLink {
    /// Where clients connect.
    pub address: SocketAddr,
    pub stats: Arc<LinkStats>,
    task: JoinHandle<()>,
    replies: Arc<Mutex<Vec<tokio::task::AbortHandle>>>,
}
impl Drop for ImpairedLink {
    fn drop(&mut self) {
        self.task.abort();
        for reply in self
            .replies
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
        {
            reply.abort();
        }
    }
}

/// Deterministic per-link randomness (xorshift64*), so a failing soak run
/// replays with the same seed.
struct Dice(u64);
impl Dice {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

struct Direction {
    impairment: Impairment,
    dice: Mutex<Dice>,
    stats: Arc<LinkStats>,
}
impl Direction {
    /// Deliver `bytes` through `send` after the link's delay, or drop it.
    fn carry(self: &Arc<Self>, bytes: Vec<u8>, socket: Arc<UdpSocket>, to: SocketAddr) {
        let (lost, copies, delays) = {
            let mut dice = self.dice.lock().unwrap_or_else(|e| e.into_inner());
            let lost = dice.next() < self.impairment.loss;
            let copies = if dice.next() < self.impairment.duplicate {
                2
            } else {
                1
            };
            let delays: Vec<Duration> = (0..copies)
                .map(|_| self.impairment.latency + self.impairment.jitter.mul_f64(dice.next()))
                .collect();
            (lost, copies, delays)
        };
        if lost {
            self.stats.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        self.stats.forwarded.fetch_add(1, Ordering::Relaxed);
        if copies > 1 {
            self.stats.duplicated.fetch_add(1, Ordering::Relaxed);
        }
        for delay in delays {
            let (bytes, socket) = (bytes.clone(), socket.clone());
            tokio::spawn(async move {
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                let _ = socket.send_to(&bytes, to).await;
            });
        }
    }
}

impl ImpairedLink {
    /// Relay to `host` on loopback with `impairment` both ways.
    pub async fn start(host: SocketAddr, impairment: Impairment, seed: u64) -> Result<Self> {
        let front = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
        let address = front.local_addr()?;
        let stats = Arc::new(LinkStats::default());
        let direction = |salt: u64| {
            Arc::new(Direction {
                impairment,
                dice: Mutex::new(Dice((seed ^ salt) | 1)),
                stats: stats.clone(),
            })
        };
        let (up, down) = (direction(0x9E37_79B9), direction(0x7F4A_7C15));
        let replies = Arc::new(Mutex::new(Vec::new()));
        let tasks = replies.clone();
        let task = tokio::spawn(async move {
            // One upstream socket per client, so the host sees distinct peers.
            let mut clients = HashMap::<SocketAddr, Arc<UdpSocket>>::new();
            let mut buffer = vec![0u8; 65_536];
            while let Ok((n, client)) = front.recv_from(&mut buffer).await {
                let upstream = match clients.get(&client) {
                    Some(socket) => socket.clone(),
                    None => {
                        let Ok(socket) = UdpSocket::bind("127.0.0.1:0").await else {
                            continue;
                        };
                        let socket = Arc::new(socket);
                        clients.insert(client, socket.clone());
                        let (reply, front, down) = (socket.clone(), front.clone(), down.clone());
                        let relay = tokio::spawn(async move {
                            let mut buffer = vec![0u8; 65_536];
                            while let Ok((n, _)) = reply.recv_from(&mut buffer).await {
                                down.carry(buffer[..n].to_vec(), front.clone(), client);
                            }
                        });
                        tasks
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .push(relay.abort_handle());
                        socket
                    }
                };
                up.carry(buffer[..n].to_vec(), upstream, host);
            }
        });
        Ok(Self {
            address,
            stats,
            task,
            replies,
        })
    }
}
