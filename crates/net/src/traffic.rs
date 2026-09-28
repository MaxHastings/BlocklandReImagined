//! What the host sends, by kind: payload bytes (after compression, before
//! QUIC framing) summed over every recipient. The bandwidth audit and its
//! budget tests read it; see `docs/audits/network-bandwidth.md`.
use std::sync::atomic::{AtomicU64, Ordering};

/// One kind of host traffic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// Player pose datagrams.
    Pose,
    /// Vehicle pose datagrams.
    Vehicle,
    /// Admin camera orb datagrams.
    Orb,
    /// Reliable world updates (`Message::Update`).
    Update,
    /// Welcome and map change checkpoints with their brick chunks.
    World,
    /// Admin snapshots.
    Admin,
    /// Per-viewer package state.
    Package,
    /// Private notices.
    Notice,
    /// Command replies.
    Reply,
}
impl Kind {
    pub const ALL: [Kind; 9] = [
        Kind::Pose,
        Kind::Vehicle,
        Kind::Orb,
        Kind::Update,
        Kind::World,
        Kind::Admin,
        Kind::Package,
        Kind::Notice,
        Kind::Reply,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Kind::Pose => "pose datagrams",
            Kind::Vehicle => "vehicle datagrams",
            Kind::Orb => "orb datagrams",
            Kind::Update => "world updates",
            Kind::World => "world transfers",
            Kind::Admin => "admin snapshots",
            Kind::Package => "package state",
            Kind::Notice => "notices",
            Kind::Reply => "replies",
        }
    }
}
/// Running totals since the host started.
#[derive(Default)]
pub struct Traffic {
    bytes: [AtomicU64; Kind::ALL.len()],
    messages: [AtomicU64; Kind::ALL.len()],
}
/// A copy of the totals; subtract two to get a window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrafficSample {
    pub bytes: [u64; Kind::ALL.len()],
    pub messages: [u64; Kind::ALL.len()],
}
impl Traffic {
    pub(crate) fn add(&self, kind: Kind, bytes: usize, recipients: usize) {
        let i = kind as usize;
        self.bytes[i].fetch_add((bytes * recipients) as u64, Ordering::Relaxed);
        self.messages[i].fetch_add(recipients as u64, Ordering::Relaxed);
    }
    pub fn sample(&self) -> TrafficSample {
        TrafficSample {
            bytes: std::array::from_fn(|i| self.bytes[i].load(Ordering::Relaxed)),
            messages: std::array::from_fn(|i| self.messages[i].load(Ordering::Relaxed)),
        }
    }
}
impl TrafficSample {
    pub fn since(&self, earlier: &TrafficSample) -> TrafficSample {
        TrafficSample {
            bytes: std::array::from_fn(|i| self.bytes[i] - earlier.bytes[i]),
            messages: std::array::from_fn(|i| self.messages[i] - earlier.messages[i]),
        }
    }
    pub fn bytes(&self, kind: Kind) -> u64 {
        self.bytes[kind as usize]
    }
    pub fn messages(&self, kind: Kind) -> u64 {
        self.messages[kind as usize]
    }
    pub fn total(&self) -> u64 {
        self.bytes.iter().sum()
    }
}
