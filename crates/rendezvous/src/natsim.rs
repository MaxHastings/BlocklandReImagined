//! Simulated home routers for headless tests of hole punching and relay.
//!
//! A [`NatSocket`] is a UDP socket "behind" a [`Nat`]: a QUIC endpoint uses
//! it through quinn's `AsyncUdpSocket`, and every datagram it sends leaves
//! from a real loopback socket on the NAT's public address (a mapping), as
//! a router would rewrite it. Replies are let back in only as the NAT's
//! filtering rule allows (RFC 4787 behaviours). Public addresses are
//! loopback addresses such as 127.0.0.2, so no privileges or network
//! namespaces are needed.
use std::{
    collections::{HashSet, VecDeque},
    fmt,
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU16, AtomicU64, Ordering},
    },
    task::{Context, Poll, Waker},
};

/// How a NAT picks the outside port for a new destination (RFC 4787 4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mapping {
    /// One outside port per inside socket, whatever the destination (most
    /// home routers).
    EndpointIndependent,
    /// A new outside port per destination ("symmetric" NAT, some carrier
    /// and corporate NATs).
    AddressAndPortDependent,
}

/// Which outside senders a mapping lets in (RFC 4787 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filtering {
    /// Anyone ("full cone").
    EndpointIndependent,
    /// Only IPs the inside socket has sent to.
    AddressDependent,
    /// Only exact addresses the inside socket has sent to.
    AddressAndPortDependent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Behaviour {
    pub mapping: Mapping,
    pub filtering: Filtering,
}
impl Behaviour {
    pub const FULL_CONE: Self = Self {
        mapping: Mapping::EndpointIndependent,
        filtering: Filtering::EndpointIndependent,
    };
    /// The common home router.
    pub const PORT_RESTRICTED: Self = Self {
        mapping: Mapping::EndpointIndependent,
        filtering: Filtering::AddressAndPortDependent,
    };
    pub const SYMMETRIC: Self = Self {
        mapping: Mapping::AddressAndPortDependent,
        filtering: Filtering::AddressAndPortDependent,
    };
}

/// A router with one public address.
#[derive(Debug)]
pub struct Nat {
    pub public_ip: IpAddr,
    pub behaviour: Behaviour,
    next_inside: AtomicU16,
    /// Datagrams from outside that the filter refused.
    pub filtered: AtomicU64,
}
impl Nat {
    pub fn new(public_ip: IpAddr, behaviour: Behaviour) -> Arc<Self> {
        Arc::new(Self {
            public_ip,
            behaviour,
            next_inside: AtomicU16::new(40000),
            filtered: AtomicU64::new(0),
        })
    }
    /// A new socket inside this NAT.
    pub fn socket(self: &Arc<Self>) -> Arc<NatSocket> {
        let port = self.next_inside.fetch_add(1, Ordering::Relaxed);
        Arc::new(NatSocket {
            nat: self.clone(),
            inside: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)), port),
            mappings: Mutex::new(Vec::new()),
            inbox: Arc::new(Mutex::new(Inbox::default())),
        })
    }
}

#[derive(Default)]
struct Inbox {
    queue: VecDeque<(SocketAddr, Vec<u8>)>,
    waker: Option<Waker>,
}

struct MappingEntry {
    /// The destination this mapping is for, or None when shared by all.
    destination: Option<SocketAddr>,
    /// Sends go straight through the OS socket: a tokio socket reports
    /// WouldBlock until its reactor has seen the socket writable once.
    sender: Arc<std::net::UdpSocket>,
    /// Addresses the inside socket has sent to through this mapping.
    sent_to: Arc<Mutex<HashSet<SocketAddr>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for MappingEntry {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// A UDP socket behind a simulated NAT, for `quinn::Endpoint::new_with_abstract_socket`.
pub struct NatSocket {
    nat: Arc<Nat>,
    inside: SocketAddr,
    mappings: Mutex<Vec<MappingEntry>>,
    inbox: Arc<Mutex<Inbox>>,
}
impl fmt::Debug for NatSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NatSocket({} behind {})", self.inside, self.nat.public_ip)
    }
}

impl NatSocket {
    /// Outside addresses this socket has been given so far.
    pub fn public_addresses(&self) -> Vec<SocketAddr> {
        self.mappings
            .lock()
            .unwrap()
            .iter()
            .filter_map(|m| m.sender.local_addr().ok())
            .collect()
    }
    fn mapping_for(&self, destination: SocketAddr) -> io::Result<(Arc<std::net::UdpSocket>, Arc<Mutex<HashSet<SocketAddr>>>)> {
        let key = match self.nat.behaviour.mapping {
            Mapping::EndpointIndependent => None,
            Mapping::AddressAndPortDependent => Some(destination),
        };
        let mut mappings = self.mappings.lock().unwrap();
        if let Some(entry) = mappings.iter().find(|m| m.destination == key) {
            return Ok((entry.sender.clone(), entry.sent_to.clone()));
        }
        let std = std::net::UdpSocket::bind((self.nat.public_ip, 0))?;
        std.set_nonblocking(true)?;
        let sender = Arc::new(std.try_clone()?);
        let socket = Arc::new(tokio::net::UdpSocket::from_std(std)?);
        let sent_to = Arc::new(Mutex::new(HashSet::new()));
        let task = tokio::spawn(receive(
            socket.clone(),
            sent_to.clone(),
            self.nat.clone(),
            self.inbox.clone(),
        ));
        mappings.push(MappingEntry {
            destination: key,
            sender: sender.clone(),
            sent_to: sent_to.clone(),
            task,
        });
        Ok((sender, sent_to))
    }
}

/// Datagrams arriving at one mapping, let in by the NAT's filter.
async fn receive(
    socket: Arc<tokio::net::UdpSocket>,
    sent_to: Arc<Mutex<HashSet<SocketAddr>>>,
    nat: Arc<Nat>,
    inbox: Arc<Mutex<Inbox>>,
) {
    let mut buffer = vec![0u8; 65536];
    loop {
        let (len, from) = match socket.recv_from(&mut buffer).await {
            Ok(received) => received,
            // An ICMP error from an earlier send; never spin on it.
            Err(_) => {
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                continue;
            }
        };
        let allowed = {
            let sent = sent_to.lock().unwrap();
            match nat.behaviour.filtering {
                Filtering::EndpointIndependent => true,
                Filtering::AddressDependent => sent.iter().any(|a| a.ip() == from.ip()),
                Filtering::AddressAndPortDependent => sent.contains(&from),
            }
        };
        if !allowed {
            nat.filtered.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        let mut inbox = inbox.lock().unwrap();
        inbox.queue.push_back((from, buffer[..len].to_vec()));
        if let Some(waker) = inbox.waker.take() {
            waker.wake();
        }
    }
}

#[derive(Debug)]
struct Writable;
impl quinn::UdpPoller for Writable {
    fn poll_writable(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl quinn::AsyncUdpSocket for NatSocket {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn quinn::UdpPoller>> {
        Box::pin(Writable)
    }
    fn try_send(&self, transmit: &quinn::udp::Transmit) -> io::Result<()> {
        let (socket, sent_to) = self.mapping_for(transmit.destination)?;
        sent_to.lock().unwrap().insert(transmit.destination);
        let segment = transmit.segment_size.unwrap_or(transmit.contents.len()).max(1);
        for datagram in transmit.contents.chunks(segment) {
            match socket.send_to(datagram, transmit.destination) {
                Ok(_) => {}
                // A full loopback buffer drops the datagram, like a network.
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
    fn poll_recv(
        &self,
        cx: &mut Context,
        bufs: &mut [io::IoSliceMut<'_>],
        meta: &mut [quinn::udp::RecvMeta],
    ) -> Poll<io::Result<usize>> {
        let mut inbox = self.inbox.lock().unwrap();
        let mut count = 0;
        while count < bufs.len().min(meta.len()) {
            let Some((from, datagram)) = inbox.queue.pop_front() else {
                break;
            };
            let len = datagram.len().min(bufs[count].len());
            bufs[count][..len].copy_from_slice(&datagram[..len]);
            meta[count] = quinn::udp::RecvMeta {
                addr: from,
                len,
                stride: len,
                ecn: None,
                dst_ip: None,
            };
            count += 1;
        }
        if count > 0 {
            return Poll::Ready(Ok(count));
        }
        inbox.waker = Some(cx.waker().clone());
        Poll::Pending
    }
    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.inside)
    }
    fn may_fragment(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quinn::AsyncUdpSocket;
    fn transmit(to: SocketAddr, contents: &[u8]) -> quinn::udp::Transmit<'_> {
        quinn::udp::Transmit {
            destination: to,
            ecn: None,
            contents,
            segment_size: None,
            src_ip: None,
        }
    }
    async fn next(socket: &NatSocket) -> Option<(SocketAddr, Vec<u8>)> {
        for _ in 0..40 {
            if let Some(item) = socket.inbox.lock().unwrap().queue.pop_front() {
                return Some(item);
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        None
    }
    #[tokio::test]
    async fn port_restricted_nats_admit_only_addresses_sent_to() {
        let nat = Nat::new("127.0.0.2".parse().unwrap(), Behaviour::PORT_RESTRICTED);
        let inside = nat.socket();
        let friend = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let stranger = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let friend_at = friend.local_addr().unwrap();
        inside.try_send(&transmit(friend_at, b"hi")).unwrap();
        let mut buffer = [0u8; 16];
        let (len, public) = friend.recv_from(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..len], b"hi");
        assert_eq!(public.ip(), nat.public_ip, "the source is rewritten");
        friend.send_to(b"back", public).await.unwrap();
        stranger.send_to(b"sneak", public).await.unwrap();
        assert_eq!(next(&inside).await, Some((friend_at, b"back".to_vec())));
        assert_eq!(next(&inside).await, None);
        assert_eq!(nat.filtered.load(Ordering::Relaxed), 1);
    }
    #[tokio::test]
    async fn symmetric_nats_use_a_new_port_per_destination() {
        let nat = Nat::new("127.0.0.3".parse().unwrap(), Behaviour::SYMMETRIC);
        let inside = nat.socket();
        let a = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let b = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        inside.try_send(&transmit(a.local_addr().unwrap(), b"x")).unwrap();
        inside.try_send(&transmit(b.local_addr().unwrap(), b"x")).unwrap();
        let mut buffer = [0u8; 4];
        let (_, via_a) = a.recv_from(&mut buffer).await.unwrap();
        let (_, via_b) = b.recv_from(&mut buffer).await.unwrap();
        assert_ne!(via_a.port(), via_b.port());
        assert_eq!(inside.public_addresses().len(), 2);
    }
}
