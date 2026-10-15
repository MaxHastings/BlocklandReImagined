//! An impaired network link for soak and stress tests: a UDP relay between
//! QUIC clients and a host that adds latency, jitter, loss, duplication and
//! a bandwidth limit in both directions. Clients connect to [`ImpairedLink::address`] instead of
//! the host; everything else is the real transport. Jitter larger than the
//! gap between packets reorders them, as the Internet does.
use anyhow::Result;
use std::{
    cmp::{Ordering as CmpOrdering, Reverse},
    collections::{BinaryHeap, HashMap},
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{net::UdpSocket, sync::mpsc, task::JoinHandle};

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

struct Delivery {
    deadline: tokio::time::Instant,
    order: u64,
    bytes: Vec<u8>,
    socket: Arc<UdpSocket>,
    to: SocketAddr,
}
impl PartialEq for Delivery {
    fn eq(&self, other: &Self) -> bool {
        (self.deadline, self.order) == (other.deadline, other.order)
    }
}
impl Eq for Delivery {}
impl PartialOrd for Delivery {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}
impl Ord for Delivery {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        (self.deadline, self.order).cmp(&(other.deadline, other.order))
    }
}

// One scheduler owns each direction's ordered deadlines. Independent sleeping
// tasks can wake out of order on a coarse timer, inventing jitter and QUIC loss.
async fn deliver(mut incoming: mpsc::Receiver<Delivery>, stats: Arc<LinkStats>) {
    let mut pending = BinaryHeap::<Reverse<Delivery>>::new();
    loop {
        if let Some(Reverse(next)) = pending.peek() {
            let deadline = next.deadline;
            tokio::select! {
                delivery=incoming.recv()=>match delivery {
                    Some(delivery) if pending.len()<4096=>pending.push(Reverse(delivery)),
                    Some(_)=>{stats.dropped.fetch_add(1,Ordering::Relaxed);},
                    None=>return,
                },
                _=tokio::time::sleep_until(deadline)=>{
                    let mut batch=0;
                    while pending.peek().is_some_and(|d|d.0.deadline<=tokio::time::Instant::now()) {
                        let Reverse(delivery)=pending.pop().unwrap();
                        if delivery.socket.send_to(&delivery.bytes,delivery.to).await.is_err() {stats.dropped.fetch_add(1,Ordering::Relaxed);}
                        batch+=1;
                        // Let socket readers drain timer-coalesced bursts too.
                        if batch%16==0 {tokio::task::yield_now().await;}
                    }
                }
            }
        } else {
            match incoming.recv().await {
                Some(delivery) => pending.push(Reverse(delivery)),
                None => return,
            }
        }
    }
}
struct Direction {
    deliveries: mpsc::Sender<Delivery>,
    order: AtomicU64,
    impairment: Impairment,
    dice: Mutex<Dice>,
    stats: Arc<LinkStats>,
    /// When the throttled link has sent everything queued so far.
    busy_until: Mutex<Option<tokio::time::Instant>>,
}
impl Direction {
    /// Deliver `bytes` through `send` after the link's delay, or drop it.
    fn carry(self: &Arc<Self>, bytes: Vec<u8>, socket: Arc<UdpSocket>, to: SocketAddr) {
        let received = tokio::time::Instant::now();
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
        let mut ready = received;
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
            ready = start + sending;
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
            let delivery = Delivery {
                deadline: ready + delay,
                order: self.order.fetch_add(1, Ordering::Relaxed),
                bytes: bytes.clone(),
                socket: socket.clone(),
                to,
            };
            if self.deliveries.try_send(delivery).is_err() {
                self.stats.dropped.fetch_add(1, Ordering::Relaxed);
            }
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
            let (deliveries, incoming) = mpsc::channel(4096);
            let worker = tokio::spawn(deliver(incoming, stats.clone()));
            (
                Arc::new(Direction {
                    deliveries,
                    order: AtomicU64::new(0),
                    impairment,
                    dice: Mutex::new(Dice((seed ^ salt) | 1)),
                    stats: stats.clone(),
                    busy_until: Mutex::new(None),
                }),
                worker.abort_handle(),
            )
        };
        let ((up, up_worker), (down, down_worker)) =
            (direction(0x9E37_79B9), direction(0x7F4A_7C15));
        let replies = Arc::new(Mutex::new(vec![up_worker, down_worker]));
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

    /// Latency alone cannot introduce packet reordering. Independent timer
    /// tasks waking together must not turn a zero-jitter link into a lossy one.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn latency_without_jitter_preserves_datagram_order() -> Result<()> {
        let host = UdpSocket::bind("127.0.0.1:0").await?;
        let link = ImpairedLink::start(
            host.local_addr()?,
            Impairment {
                latency: Duration::from_millis(20),
                ..Impairment::NONE
            }
            .throttled(12_500_000, Duration::from_millis(100)),
            73,
        )
        .await?;
        let client = UdpSocket::bind("127.0.0.1:0").await?;
        let sender = async {
            for index in 0u32..32 {
                let mut bytes = vec![0u8; 500];
                bytes[..4].copy_from_slice(&index.to_le_bytes());
                client.send_to(&bytes, link.address).await?;
            }
            anyhow::Ok(())
        };
        let receiver = async {
            let mut order = Vec::new();
            let mut bytes = [0u8; 1024];
            for _ in 0..32 {
                let (n, _) =
                    tokio::time::timeout(Duration::from_secs(2), host.recv_from(&mut bytes))
                        .await??;
                anyhow::ensure!(n == 500, "relay changed the datagram");
                order.push(u32::from_le_bytes(bytes[..4].try_into().unwrap()));
            }
            anyhow::Ok(order)
        };
        let (sent, received) = tokio::join!(sender, receiver);
        sent?;
        assert_eq!(received?, (0..32).collect::<Vec<_>>());
        Ok(())
    }

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
        // Sent in bursts that fit the loopback's own buffers (64 kB on
        // Windows), with a sleep between them (at least 1 ms, about 15 ms on
        // Windows' timer) so the link reads each burst before the next even
        // on one core: what the link loses is the link's. Bursts of 10 go
        // under the link's rate on Windows; bursts of 50 overrun it
        // everywhere.
        async fn send(
            client: &UdpSocket,
            to: SocketAddr,
            datagram: &[u8],
            n: usize,
            burst: usize,
        ) -> Result<()> {
            for i in 0..n {
                client.send_to(datagram, to).await?;
                if i % burst == burst - 1 {
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
        // The host reads while the client sends: a socket nobody reads
        // overflows its receive buffer (64 kB on Windows) on its own.
        let first = async {
            let mut buffer = vec![0u8; 2048];
            tokio::time::timeout(Duration::from_secs(5), host.recv_from(&mut buffer)).await??;
            anyhow::Ok(1 + receive(&host).await)
        };
        let (sent, arrived) = tokio::join!(send(&client, link.address, &datagram, 100, 10), first);
        sent?;
        let arrived = arrived?;
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
        // 1 MB well over the link's rate: what would wait longer than
        // 200 ms is dropped, the rest arrives.
        let (sent, more) = tokio::join!(
            send(&client, link.address, &datagram, 1000, 50),
            receive(&host)
        );
        sent?;
        let arrived = arrived + more;
        let dropped = link.stats.dropped.load(Ordering::Relaxed);
        let forwarded = link.stats.forwarded.load(Ordering::Relaxed);
        assert!(dropped >= 50, "dropped {dropped} of 1000");
        // (A loaded machine's loopback can lose a few on its own.)
        assert!(
            arrived * 10 >= forwarded * 9,
            "{arrived} of the {forwarded} the link took arrived"
        );
        Ok(())
    }
}
