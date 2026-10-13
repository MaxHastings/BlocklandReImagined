//! An impaired network link for soak and stress tests: a UDP relay between
//! QUIC clients and a host that adds latency, jitter, loss, duplication and
//! a bandwidth limit in both directions. Clients connect to [`ImpairedLink::address`] instead of
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
    /// The link's bandwidth in bytes per second, 0 for unlimited: each
    /// datagram waits for the ones before it to go out, as on a throttled
    /// connection.
    pub rate: u64,
    /// The most a datagram waits for the link before it is dropped instead,
    /// as a router's buffer (only with a `rate`).
    pub queue: Duration,
}
impl Impairment {
    /// A clean link.
    pub const NONE: Self = Self {
        latency: Duration::ZERO,
        jitter: Duration::ZERO,
        loss: 0.0,
        duplicate: 0.0,
        rate: 0,
        queue: Duration::ZERO,
    };
    /// A poor but playable home connection.
    pub const BAD_WIFI: Self = Self {
        latency: Duration::from_millis(60),
        jitter: Duration::from_millis(40),
        loss: 0.05,
        duplicate: 0.01,
        rate: 0,
        queue: Duration::ZERO,
    };
    /// `self` limited to `rate` bytes per second with a router's buffer of
    /// `queue`.
    pub const fn throttled(self, rate: u64, queue: Duration) -> Self {
        Self {
            rate,
            queue,
            ..self
        }
    }
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
    /// When the throttled link has sent everything queued so far.
    busy_until: Mutex<Option<tokio::time::Instant>>,
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
        // The bandwidth limit: out after everything queued before it, or
        // dropped when that is more than the buffer holds.
        let mut wait = Duration::ZERO;
        if !lost && self.impairment.rate > 0 {
            let now = tokio::time::Instant::now();
            let mut busy = self.busy_until.lock().unwrap_or_else(|e| e.into_inner());
            let start = busy.map_or(now, |b| b.max(now));
            if start - now > self.impairment.queue {
                self.stats.dropped.fetch_add(1, Ordering::Relaxed);
                return;
            }
            let sending = Duration::from_secs_f64(bytes.len() as f64 / self.impairment.rate as f64);
            *busy = Some(start + sending);
            wait = start + sending - now;
        }
        if lost {
            self.stats.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        self.stats.forwarded.fetch_add(1, Ordering::Relaxed);
        if copies > 1 {
            self.stats.duplicated.fetch_add(1, Ordering::Relaxed);
        }
        for delay in delays {
            let delay = delay + wait;
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
                busy_until: Mutex::new(None),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A throttled link takes as long as its bandwidth says, and its buffer
    /// drops what does not fit.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_throttled_link_paces_and_drops_past_its_buffer() -> Result<()> {
        let host = UdpSocket::bind("127.0.0.1:0").await?;
        let rate = 1_000_000;
        let link = ImpairedLink::start(
            host.local_addr()?,
            Impairment::NONE.throttled(rate, Duration::from_millis(200)),
            1,
        )
        .await?;
        let client = UdpSocket::bind("127.0.0.1:0").await?;
        let datagram = vec![7u8; 1000];
        // Sent in small bursts, so the loopback's own buffers never drop
        // any: what the link loses is the link's.
        async fn send(client: &UdpSocket, to: SocketAddr, datagram: &[u8], n: usize) -> Result<()> {
            for i in 0..n {
                client.send_to(datagram, to).await?;
                if i % 10 == 9 {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }
            Ok(())
        }
        async fn receive(host: &UdpSocket) -> u64 {
            let mut buffer = vec![0u8; 2048];
            let mut arrived = 0;
            while tokio::time::timeout(Duration::from_millis(500), host.recv_from(&mut buffer))
                .await
                .is_ok()
            {
                arrived += 1;
            }
            arrived
        }
        let start = std::time::Instant::now();
        // 100 kB: a tenth of a second at 1 MB/s, within the buffer.
        send(&client, link.address, &datagram, 100).await?;
        let mut buffer = vec![0u8; 2048];
        tokio::time::timeout(Duration::from_secs(5), host.recv_from(&mut buffer)).await??;
        let arrived = 1 + receive(&host).await;
        let took = start.elapsed() - Duration::from_millis(500);
        let forwarded = link.stats.forwarded.load(Ordering::Relaxed);
        assert_eq!(link.stats.dropped.load(Ordering::Relaxed), 0);
        assert!(
            arrived * 10 >= forwarded * 9,
            "{arrived} of {forwarded} arrived"
        );
        assert!(forwarded >= 90, "{forwarded} of 100 reached the link");
        let least = Duration::from_secs_f64((forwarded - 1) as f64 * 1000.0 / rate as f64);
        assert!(took >= least, "{forwarded} kB at 1 MB/s took {took:?}");
        // 500 kB as fast as the client sends it: what would wait longer
        // than 200 ms is dropped, the rest arrives.
        send(&client, link.address, &datagram, 500).await?;
        let arrived = arrived + receive(&host).await;
        let dropped = link.stats.dropped.load(Ordering::Relaxed);
        let forwarded = link.stats.forwarded.load(Ordering::Relaxed);
        assert!(dropped >= 50, "dropped {dropped} of 500");
        // (A loaded machine's loopback can lose a few on its own.)
        assert!(
            arrived * 10 >= forwarded * 9,
            "{arrived} of the {forwarded} the link took arrived"
        );
        Ok(())
    }
}
