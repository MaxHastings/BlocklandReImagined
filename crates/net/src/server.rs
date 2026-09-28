use crate::{
    admin_store::AdminStore,
    codec,
    protocol::*,
    traffic::{Kind, Traffic},
};
use anyhow::{Context, Result, ensure};
use bri_admin::Principal;
use bri_sim::session::Session;
use bri_world::OwnerId;
use glam::Vec3;
use quinn::{Connection, Endpoint};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Semaphore, mpsc, oneshot, watch};

pub struct ServerOptions {
    pub bind: SocketAddr,
    /// Every package this server loaded; joining clients must agree on the
    /// shared ones.
    pub environment: bri_package::environment::Environment,
    pub spawn_points: Vec<Vec3>,
    /// A persistent host identity lets joiners keep trusting this host
    /// across restarts. None generates a throwaway certificate.
    pub certificate: Option<HostCertificate>,
    /// Builds a configured, empty session for a map id (admin Change Map).
    pub map_loader: Option<MapLoader>,
    /// Periodic durable checkpoints of the authoritative world, so a crash
    /// loses at most one interval. None keeps state only in memory.
    pub autosave: Option<Autosave>,
    /// Packages clients may download before joining. None offers nothing.
    pub packages: Option<Arc<crate::packages::PackageShelf>>,
}
/// The host hands a snapshot of its world to `save` every `every`, on a
/// blocking thread and never two at once, and once more when the host loop
/// ends with an error (a clean stop returns the world in its report).
#[derive(Clone)]
pub struct Autosave {
    pub every: Duration,
    pub save: SaveWorld,
}
/// Writes one world snapshot durably; runs on a blocking thread.
pub type SaveWorld = Arc<dyn Fn(&bri_world::World) -> Result<()> + Send + Sync>;
/// What differs between a joiner's packages and the server's, for the
/// joining player to be told about.
struct PackageDifferences {
    /// Presentation only: things may look or sound different.
    cosmetic: Vec<bri_package::environment::Mismatch>,
    /// Shared content the joiner could not get from this server (never
    /// offered, or its download failed) and joined without.
    unavailable: Vec<bri_package::environment::Mismatch>,
}
/// Refuse a first join whose shared packages differ from the server's,
/// naming every differing package, so the joiner downloads them. A joiner
/// that already fetched what this server offers (`accept_differences`) is
/// let in with whatever still differs, and told what it lacks: a join never
/// fails over Add-Ons. Presentation-only differences are always allowed.
fn check_packages(
    environment: &bri_package::environment::Environment,
    client: &[bri_package::environment::PackageRef],
    accept_differences: bool,
) -> Result<PackageDifferences> {
    let (blocking, cosmetic): (Vec<_>, Vec<_>) = environment
        .compare(client)
        .into_iter()
        .partition(|m| m.blocks_join());
    if !blocking.is_empty() && !accept_differences {
        return Err(crate::client::PackagesDiffer(blocking).into());
    }
    Ok(PackageDifferences {
        cosmetic,
        unavailable: blocking,
    })
}
/// Loads a map for Change Map; runs on a blocking thread.
pub type MapLoader = Arc<dyn Fn(&str) -> Result<Session> + Send + Sync>;
/// Self-signed QUIC host certificate and its PKCS#8 private key.
#[derive(Clone)]
pub struct HostCertificate {
    pub der: Vec<u8>,
    pub key: Vec<u8>,
}
impl HostCertificate {
    pub fn generate() -> Result<Self> {
        let cert = rcgen::generate_simple_self_signed(vec!["blockland.local".into()])?;
        Ok(Self {
            der: cert.cert.der().to_vec(),
            key: cert.signing_key.serialize_der(),
        })
    }
    /// Load `host-identity.bin` (certificate and key in one file, so a crash
    /// can never pair a certificate with the wrong key) from a private state
    /// directory, creating it on first use. A damaged file is reported, not
    /// replaced: a new certificate would break every friend's saved pin.
    pub fn load_or_create(dir: &std::path::Path) -> Result<Self> {
        let path = dir.join("host-identity.bin");
        match std::fs::read(&path) {
            Ok(bytes) => {
                return Self::decode(&bytes).with_context(|| {
                    format!(
                        "The host certificate file {} is damaged; move it aside to create a new one                          (friends will then need to trust this host again)",
                        path.display()
                    )
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("Could not read the host certificate"),
        }
        let identity = Self::generate()?;
        bri_files::create_new_private(&path, &identity.encode())
            .context("Could not save the host certificate")?;
        Ok(identity)
    }
    const MAGIC: &[u8; 8] = b"BRIHOST1";
    fn encode(&self) -> Vec<u8> {
        let mut out = Self::MAGIC.to_vec();
        for part in [&self.der, &self.key] {
            out.extend((part.len() as u32).to_le_bytes());
            out.extend(part.iter());
        }
        out
    }
    fn decode(bytes: &[u8]) -> Result<Self> {
        let rest = bytes
            .strip_prefix(Self::MAGIC.as_slice())
            .context("Not a host certificate file")?;
        let mut parts = Vec::new();
        let mut rest = rest;
        for _ in 0..2 {
            ensure!(rest.len() >= 4, "Truncated host certificate file");
            let (length, tail) = rest.split_at(4);
            let length = u32::from_le_bytes(length.try_into()?) as usize;
            ensure!(
                (1..=16384).contains(&length) && tail.len() >= length,
                "Invalid host certificate length"
            );
            parts.push(tail[..length].to_vec());
            rest = &tail[length..];
        }
        ensure!(rest.is_empty(), "Trailing bytes in host certificate file");
        let key = parts.pop().unwrap();
        let der = parts.pop().unwrap();
        Ok(Self { der, key })
    }
}
pub struct ServerHandle {
    pub address: SocketAddr,
    pub certificate: Vec<u8>,
    /// Private in-process host capability. Never write this into public host metadata.
    pub host_token: ResumeToken,
    /// Live connected-player count (LAN listing).
    pub players: Arc<std::sync::atomic::AtomicU32>,
    /// What probes and the join list see; `players` is filled in live.
    listing: Arc<std::sync::Mutex<Listing>>,
    /// The host's own performance, refreshed about once a second.
    pub perf: Arc<Mutex<ServerPerf>>,
    /// What the host has sent, by kind.
    pub traffic: Arc<Traffic>,
    discovery: Option<tokio::task::JoinHandle<()>>,
    router: Option<RouterPorts>,
    stop: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<Result<ServerReport>>,
}
/// How the host's simulation is keeping up, for the host's performance
/// overlay. Measured on the host only; never sent to players.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ServerPerf {
    /// Simulation steps per second over the last window (120 when keeping up).
    pub ticks_per_second: f32,
    /// Wall-clock time of one simulation step: mean and worst in the window.
    pub tick_ms_mean: f32,
    pub tick_ms_max: f32,
    /// Script time per Add-On package, averaged per step, busiest first.
    pub script_ms: Vec<(String, f32)>,
    pub players: u32,
}
/// Collects step times until a window (about a second) closes.
#[derive(Default)]
struct PerfWindow {
    started: Option<std::time::Instant>,
    steps: u32,
    total: Duration,
    max: Duration,
}
impl PerfWindow {
    const LENGTH: Duration = Duration::from_secs(1);
    fn step(&mut self, took: Duration) {
        self.steps += 1;
        self.total += took;
        self.max = self.max.max(took);
    }
    /// The window's summary once it has run its length, then a new window.
    fn finish(
        &mut self,
        now: std::time::Instant,
        script: BTreeMap<String, Duration>,
        players: u32,
    ) -> Option<ServerPerf> {
        let started = *self.started.get_or_insert(now);
        let span = now.saturating_duration_since(started);
        if span < Self::LENGTH {
            return None;
        }
        let steps = self.steps.max(1) as f32;
        let ms = |d: Duration| d.as_secs_f32() * 1000.0;
        let mut script_ms: Vec<(String, f32)> =
            script.into_iter().map(|(id, t)| (id, ms(t) / steps)).collect();
        script_ms.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let perf = ServerPerf {
            ticks_per_second: self.steps as f32 / span.as_secs_f32(),
            tick_ms_mean: ms(self.total) / steps,
            tick_ms_max: ms(self.max),
            script_ms,
            players,
        };
        *self = PerfWindow {
            started: Some(now),
            ..Default::default()
        };
        Some(perf)
    }
}
#[derive(Debug, Serialize)]
pub struct ServerReport {
    pub weapon_adapter_gaps: BTreeMap<String, u64>,
    pub ticks: u64,
    /// Ticks where a gameplay system failed and was contained.
    pub step_errors: u64,
    pub dropped_ticks: u64,
    pub dropped_cues: u64,
    pub joins: u64,
    pub resumes: u64,
    pub commands: u64,
    pub rejected: u64,
    pub final_world: PublicWorld,
    /// Completed and failed autosaves.
    pub autosaves: u64,
    pub autosave_failures: u64,
    #[serde(skip)]
    pub native_world: bri_world::World,
    pub notices: Vec<String>,
    /// Durable package state and world edits for the host to save.
    #[serde(skip)]
    pub packages: Option<bri_sim::session::PackageSave>,
    pub package_diagnostics: Vec<bri_package::diag::Diagnostic>,
    pub package_stats: bri_sim::session::PackageStats,
}
impl ServerHandle {
    /// Answer LAN discovery queries for this host until it stops.
    pub async fn advertise(&mut self, name: String, map: String, max_players: u32, content_id: String) -> Result<()> {
        // Tests set BRI_TEST_DISCOVERY_PORT (0 picks a free port) so they
        // never collide with a game hosting on this machine.
        let discovery = std::env::var("BRI_TEST_DISCOVERY_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(crate::discovery::DISCOVERY_PORT);
        self.advertise_on(discovery, name, map, max_players, content_id).await?;
        Ok(())
    }
    /// Answer LAN queries on `discovery_port` (0 picks a free port, for tests
    /// that must not collide with a running host). Returns the bound port.
    pub async fn advertise_on(
        &mut self,
        discovery_port: u16,
        name: String,
        map: String,
        max_players: u32,
        content_id: String,
    ) -> Result<u16> {
        if let Ok(mut listing) = self.listing.lock() {
            listing.name = name.clone();
            listing.map = map.clone();
            listing.max_players = max_players;
        }
        let beacon = crate::discovery::Beacon {
            version: VERSION,
            name,
            port: self.address.port(),
            players: 0,
            max_players,
            map,
            content_id,
            certificate: crate::discovery::hex(&self.certificate),
        };
        let (task, port) =
            crate::discovery::respond(beacon, self.players.clone(), discovery_port).await?;
        self.discovery = Some(task);
        Ok(port)
    }
    /// Internet hosts: ask the router to forward the game port for as long
    /// as this host runs, then check whether friends can reach it. The
    /// report is handed to `notify` once. LAN discovery (UDP 28050) is never
    /// forwarded: joining over the internet needs only the game port.
    pub fn open_to_internet(&mut self, notify: impl FnOnce(crate::reach::Report) + Send + 'static) {
        let port = self.address.port();
        let certificate = self.certificate.clone();
        let slot = Arc::new(std::sync::Mutex::new(None::<crate::reach::Forward>));
        let held = slot.clone();
        let task = tokio::spawn(async move {
            let (report, forward) = crate::reach::open_and_check(port, certificate).await;
            notify(report);
            let Some(forward) = forward else { return };
            if let Ok(mut guard) = held.lock() {
                *guard = Some(forward);
            }
            loop {
                tokio::time::sleep(crate::upnp::RENEW_EVERY).await;
                let held = held.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    if let Ok(mut guard) = held.lock()
                        && let Some(forward) = guard.as_mut()
                    {
                        let _ = forward.renew();
                    }
                })
                .await;
            }
        });
        self.router = Some(RouterPorts { task, slot });
    }
    pub async fn stop(mut self) -> Result<ServerReport> {
        self.router.take();
        if let Some(discovery) = self.discovery.take() {
            discovery.abort();
        }
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        self.task.await?
    }
}
/// The listing with the live player count.
fn current_listing(
    listing: &std::sync::Mutex<Listing>,
    players: &std::sync::atomic::AtomicU32,
) -> Listing {
    let mut listing = listing.lock().map(|l| l.clone()).unwrap_or_default();
    listing.players = players.load(std::sync::atomic::Ordering::Relaxed);
    listing
}
/// Router forwards held for a running internet host. Dropping this removes
/// them on a background thread so the caller never waits on the router.
struct RouterPorts {
    task: tokio::task::JoinHandle<()>,
    slot: Arc<std::sync::Mutex<Option<crate::reach::Forward>>>,
}
impl Drop for RouterPorts {
    fn drop(&mut self) {
        self.task.abort();
        let slot = self.slot.clone();
        std::thread::spawn(move || drop(slot.lock().ok().and_then(|mut m| m.take())));
    }
}
/// A world transfer's encoded frames, or why encoding failed.
type EncodedTransfer = Option<Result<Arc<[Vec<u8>]>, String>>;
/// One entry in a peer's ordered reliable stream.
#[derive(Clone)]
enum Frame {
    Ready(Arc<Vec<u8>>),
    /// Frames still being encoded on a blocking thread (a world transfer).
    /// The writer waits for them in place, so later frames stay behind them.
    Pending(watch::Receiver<EncodedTransfer>),
}
/// Encode a world transfer off the authority loop. Every peer given the
/// returned frame writes the transfer at that point in its stream.
fn encode_transfer(transfer: WorldTransfer, traffic: Arc<Traffic>, recipients: usize) -> Frame {
    let (ready, frames) = watch::channel(None);
    tokio::task::spawn_blocking(move || {
        let encoded = transfer
            .encode()
            .map(Arc::from)
            .map_err(|error| format!("{error:#}"));
        if let Ok(frames) = &encoded {
            let frames: &Arc<[Vec<u8>]> = frames;
            traffic.add(Kind::World, frames.iter().map(Vec::len).sum(), recipients);
        }
        let _ = ready.send(Some(encoded));
    });
    Frame::Pending(frames)
}
/// Frames a peer's writer has not sent yet. Bounded in frames and in bytes:
/// a joining peer receives a legal world of up to the transfer budget while
/// every tick's deltas queue behind it, so the bound must be generous in
/// count and firm in bytes (stress campaign W7).
const RELIABLE_BACKLOG_FRAMES: usize = 8192;
const RELIABLE_BACKLOG_BYTES: usize = 64 * 1024 * 1024;
#[derive(Clone)]
struct Outbox {
    frames: mpsc::Sender<Frame>,
    bytes: Arc<std::sync::atomic::AtomicUsize>,
}
fn outbox() -> (Outbox, mpsc::Receiver<Frame>, Arc<std::sync::atomic::AtomicUsize>) {
    let (frames, receiver) = mpsc::channel::<Frame>(RELIABLE_BACKLOG_FRAMES);
    let bytes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    (
        Outbox {
            frames,
            bytes: bytes.clone(),
        },
        receiver,
        bytes,
    )
}
impl Outbox {
    fn try_send(&self, frame: Frame) -> Result<()> {
        let size = match &frame {
            Frame::Ready(bytes) => bytes.len(),
            Frame::Pending(_) => 0,
        };
        let queued = self.bytes.fetch_add(size, Ordering::Relaxed) + size;
        if queued > RELIABLE_BACKLOG_BYTES || self.frames.try_send(frame).is_err() {
            self.bytes.fetch_sub(size, Ordering::Relaxed);
            anyhow::bail!("Reliable backlog exceeded");
        }
        Ok(())
    }
}
struct Peer {
    connection: Connection,
    out: Outbox,
    traffic: Arc<Traffic>,
    generation: usize,
    /// May send bulk requests (administrators); read by the connection task.
    bulk: Arc<AtomicBool>,
}
impl Peer {
    /// Queue a reliable frame. A peer too far behind is disconnected rather
    /// than buffered without bound.
    fn send(&self, frame: Frame) {
        if self.out.try_send(frame).is_err() {
            self.connection.close(1_u32.into(), b"Reliable backlog exceeded");
        }
    }
    /// Encode and queue a message for this peer only.
    fn send_message(&self, kind: Kind, message: &Message) {
        match codec::encode(message) {
            Ok(bytes) => {
                self.traffic.add(kind, bytes.len(), 1);
                self.send(Frame::Ready(Arc::new(bytes)))
            }
            Err(error) => {
                eprintln!("Server could not encode a message: {error:#}");
                self.connection.close(2_u32.into(), b"Host state exceeds transfer budget");
            }
        }
    }
}
/// Encode once and queue for every peer. A message that cannot be encoded
/// disconnects the peers (their replicas would diverge) but never stops the
/// host: one oversized world or report must not end the server for everyone.
fn broadcast<'a>(peers: impl IntoIterator<Item = &'a Peer>, kind: Kind, message: &Message) {
    match codec::encode(message) {
        Ok(bytes) => {
            let size = bytes.len();
            let frame = Frame::Ready(Arc::new(bytes));
            for peer in peers {
                peer.traffic.add(kind, size, 1);
                peer.send(frame.clone());
            }
        }
        Err(error) => {
            eprintln!("Server could not encode a broadcast: {error:#}");
            for peer in peers {
                peer.connection.close(2_u32.into(), b"Host state exceeds transfer budget");
            }
        }
    }
}
enum Event {
    Join {
        hello: Hello,
        principal: Option<Principal>,
        connection: Connection,
        out: Outbox,
        bulk: Arc<AtomicBool>,
        answer: oneshot::Sender<Result<OwnerId, Message>>,
    },
    Command {
        owner: OwnerId,
        generation: usize,
        request: Request,
        _body_permit: tokio::sync::OwnedSemaphorePermit,
    },
    Move {
        owner: OwnerId,
        generation: usize,
        movement: Movement,
    },
    Lost {
        owner: OwnerId,
        generation: usize,
    },
}
pub fn transport() -> quinn::TransportConfig {
    let mut t = quinn::TransportConfig::default();
    t.max_concurrent_bidi_streams(1_u32.into())
        .max_concurrent_uni_streams(0_u32.into())
        .datagram_receive_buffer_size(Some(64 * 1024))
        .datagram_send_buffer_size(64 * 1024)
        .keep_alive_interval(Some(Duration::from_secs(2)))
        .max_idle_timeout(Some(Duration::from_secs(15).try_into().unwrap()))
        // Players' links lose packets at random (Wi-Fi, mobile); loss-based
        // Cubic reads that as congestion and stalled reliable replies for
        // 3-8 s at 5% loss in the soak test. BBR paces by measured bandwidth
        // and RTT: the same link answers within ~0.5 s.
        .congestion_controller_factory(Arc::new(quinn::congestion::BbrConfig::default()));
    t
}
pub fn start(session: Session, options: ServerOptions) -> Result<ServerHandle> {
    start_with_limit(session, options, 64)
}
/// Host-selected admission limit; clients cannot override it in their hello.
pub fn start_with_limit(
    session: Session,
    options: ServerOptions,
    max_players: usize,
) -> Result<ServerHandle> {
    start_configured(session, options, max_players, None, false)
}
/// Start with validated persistent administration state and require a proved
/// local identity for every admission. The path is chosen by the host App.
pub fn start_with_admin_store(
    session: Session,
    options: ServerOptions,
    path: impl AsRef<std::path::Path>,
) -> Result<ServerHandle> {
    start_with_admin_store_and_limit(session, options, 64, path)
}
/// Persistent administration variant preserving the host-selected player cap.
pub fn start_with_admin_store_and_limit(
    mut session: Session,
    options: ServerOptions,
    max_players: usize,
    path: impl AsRef<std::path::Path>,
) -> Result<ServerHandle> {
    let (store, state) = AdminStore::open(path)?;
    let mut bytes = Vec::new();
    state.write(&mut bytes)?;
    session.restore_admin_state(&bytes)?;
    start_configured(session, options, max_players, Some(store), true)
}
fn start_configured(
    session: Session,
    options: ServerOptions,
    max_players: usize,
    admin_store: Option<AdminStore>,
    require_identity: bool,
) -> Result<ServerHandle> {
    ensure!((1..=64).contains(&max_players), "Invalid player limit");
    ensure!(
        !options.spawn_points.is_empty()
            && options.spawn_points.len() <= 256
            && options.environment.packages.len() <= bri_package::environment::MAX_PACKAGES,
        "Invalid server options"
    );
    let identity = match &options.certificate {
        Some(identity) => identity.clone(),
        None => HostCertificate::generate()?,
    };
    let certificate = identity.der.clone();
    let server_fingerprint: [u8; 32] = Sha256::digest(&certificate).into();
    let key = quinn::rustls::pki_types::PrivatePkcs8KeyDer::from(identity.key.clone());
    let mut config =
        quinn::ServerConfig::with_single_cert(vec![certificate.clone().into()], key.into())?;
    config.transport_config(Arc::new(transport()));
    let endpoint = Endpoint::server(config, options.bind)?;
    let address = endpoint.local_addr()?;
    let (stop_tx, stop_rx) = oneshot::channel();
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("OS randomness failed: {e}"))?;
    let host_token = ResumeToken(bytes);
    let host_key = token_key(&host_token);
    let players = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let listing = Arc::new(std::sync::Mutex::new(Listing {
        name: session.simulation().state().name.clone(),
        map: session.simulation().state().map_id.clone(),
        players: 0,
        max_players: max_players as u32,
    }));
    let perf = Arc::new(Mutex::new(ServerPerf::default()));
    let traffic = Arc::new(Traffic::default());
    let task = tokio::spawn(run(
        players.clone(),
        perf.clone(),
        traffic.clone(),
        listing.clone(),
        endpoint,
        session,
        options,
        max_players,
        host_key,
        server_fingerprint,
        require_identity,
        admin_store,
        stop_rx,
    ));
    Ok(ServerHandle {
        address,
        certificate,
        host_token,
        players,
        listing,
        perf,
        traffic,
        discovery: None,
        router: None,
        stop: Some(stop_tx),
        task,
    })
}
/// Keep a connection open after its last reply (a refusal, or a finished
/// download) until the peer has read it and closed, up to a few seconds.
/// Closing sooner races the peer's read: QUIC discards stream data the
/// application has not read yet, so the player saw "connection lost"
/// instead of the refusal.
async fn linger(send: &mut quinn::SendStream, connection: &Connection) {
    let _ = tokio::time::timeout(Duration::from_secs(3), async {
        let _ = send.stopped().await;
        connection.closed().await;
    })
    .await;
}
#[allow(clippy::too_many_arguments)]
async fn connection_task(
    connection: Connection,
    events: mpsc::Sender<Event>,
    bulk_budget: Arc<Semaphore>,
    handshake: HandshakeSlot,
    deadline: tokio::time::Instant,
    server_fingerprint: [u8; 32],
    require_identity: bool,
    downloads: (Option<Arc<crate::packages::PackageShelf>>, HandshakeGate),
    listing: Listing,
) -> Result<()> {
    // The whole pre-join exchange shares one deadline, so a peer cannot hold
    // its handshake slot for a timeout per step.
    let (mut send, mut receive) = tokio::time::timeout_at(deadline, connection.accept_bi()).await??;
    let begin: JoinBegin =
        tokio::time::timeout_at(deadline, codec::read_small_request(&mut receive)).await??;
    if begin.version != VERSION {
        let reason = format!(
            "This server runs a {} version of Blockland ReImagined (protocol {VERSION}, yours is {}). {}",
            if begin.version < VERSION { "newer" } else { "older" },
            begin.version,
            if begin.version < VERSION { "Update your game to join." } else { "The host needs to update to the version you have." },
        );
        codec::write_frame(&mut send, &codec::encode(&Message::Rejected(reason))?).await?;
        send.finish()?;
        linger(&mut send, &connection).await;
        return Ok(());
    }
    if begin.purpose == Purpose::Download {
        let (shelf, gate) = downloads;
        let refusal = match (shelf, gate.admit(connection.remote_address().ip())) {
            (None, _) => "This server does not offer package downloads",
            (Some(_), None) => "Too many package downloads; try again shortly",
            (Some(shelf), Some(slot)) => {
                // A download holds its own slot, not a joining one.
                drop(handshake);
                let _slot = slot;
                crate::packages::serve(shelf, &mut send, &mut receive).await?;
                linger(&mut send, &connection).await;
                return Ok(());
            }
        };
        codec::write_frame(&mut send, &codec::encode(&Message::Rejected(refusal.into()))?).await?;
        send.finish()?;
        linger(&mut send, &connection).await;
        return Ok(());
    }
    let mut nonce = [0; 32];
    getrandom::fill(&mut nonce).map_err(|error| anyhow::anyhow!("OS randomness failed: {error}"))?;
    codec::write_frame(
        &mut send,
        &codec::encode(&Message::Challenge { nonce, listing })?,
    )
    .await?;
    let hello: Hello =
        tokio::time::timeout_at(deadline, codec::read_small_request(&mut receive)).await??;
    let principal = match verify_identity(&hello, &nonce, &server_fingerprint, require_identity) {
        Ok(principal) => principal,
        Err(error) => {
            let bytes = codec::encode(&Message::Rejected(error.to_string()))?;
            let _ = codec::write_frame(&mut send, &bytes).await;
            let _ = send.finish();
            linger(&mut send, &connection).await;
            return Ok(());
        }
    };
    let (out, mut output, queued) = outbox();
    let (answer, accepted) = oneshot::channel();
    let bulk = Arc::new(AtomicBool::new(false));
    events
        .send(Event::Join {
            hello,
            principal,
            connection: connection.clone(),
            out,
            bulk: bulk.clone(),
            answer,
        })
        .await?;
    let owner = match accepted.await? {
        Ok(owner) => owner,
        Err(refusal) => {
            codec::write_frame(&mut send, &codec::encode(&refusal)?).await?;
            send.finish()?;
            linger(&mut send, &connection).await;
            return Ok(());
        }
    };
    // Admitted players are bounded by the player limit, not handshake slots.
    drop(handshake);
    let generation = connection.stable_id();
    let own_budget = Arc::new(Semaphore::new(codec::PEER_REQUEST_BUDGET));
    let write = async {
        while let Some(frame) = output.recv().await {
            match frame {
                Frame::Ready(bytes) => {
                    write_timed(&mut send, &bytes).await?;
                    queued.fetch_sub(bytes.len(), Ordering::Relaxed);
                }
                Frame::Pending(mut ready) => {
                    let encoded = ready.wait_for(Option::is_some).await?.clone();
                    let frames = encoded
                        .context("World transfer abandoned")?
                        .map_err(|error| anyhow::anyhow!("World transfer failed: {error}"))?;
                    for bytes in frames.iter() {
                        write_timed(&mut send, bytes).await?;
                    }
                }
            }
        }
        Result::<()>::Ok(())
    };
    let read = async {
        loop {
            let (request, permit) = codec::read_budgeted_request(&mut receive, |length| {
                if length <= codec::PLAYER_MAX_REQUEST {
                    return Ok((
                        own_budget.clone(),
                        length.max(codec::MIN_REQUEST_COST) as u32,
                    ));
                }
                if !bulk.load(Ordering::Relaxed) {
                    let reason = format!(
                        "A {length}-byte request exceeds the {}-byte player limit; only administrators may send bulk requests",
                        codec::PLAYER_MAX_REQUEST
                    );
                    connection.close(3_u32.into(), reason.as_bytes());
                    anyhow::bail!(reason);
                }
                Ok((bulk_budget.clone(), length as u32))
            })
            .await?;
            events
                .send(Event::Command {
                    owner,
                    generation,
                    request,
                    _body_permit: permit,
                })
                .await?;
        }
        #[allow(unreachable_code)]
        Result::<()>::Ok(())
    };
    let datagrams = async {
        // Clients send at most one movement datagram per 120 Hz prediction
        // tick. Excess is dropped here so one flooding peer cannot crowd the
        // shared event queue that every other player's commands pass through.
        let mut allowance = MovementAllowance::new(tokio::time::Instant::now());
        loop {
            let bytes = connection.read_datagram().await?;
            if !allowance.take(tokio::time::Instant::now()) {
                continue;
            }
            if let Ok(movement) = codec::decode_datagram::<Movement>(&bytes)
                && movement.validate().is_ok()
            {
                events
                    .send(Event::Move {
                        owner,
                        generation,
                        movement,
                    })
                    .await?;
            }
        }
        #[allow(unreachable_code)]
        Result::<()>::Ok(())
    };
    tokio::select! {_ = write=>{},_ = read=>{},_ = datagrams=>{}}
    connection.close(0_u32.into(), b"Session ended");
    let _ = events.send(Event::Lost { owner, generation }).await;
    Ok(())
}
/// Token bucket for one peer's movement datagrams: twice the prediction tick
/// rate sustained, with a burst for a stalled client's catch-up.
struct MovementAllowance {
    tokens: f64,
    at: tokio::time::Instant,
}
impl MovementAllowance {
    const RATE: f64 = 240.0;
    const BURST: f64 = 60.0;
    fn new(now: tokio::time::Instant) -> Self {
        Self {
            tokens: Self::BURST,
            at: now,
        }
    }
    fn take(&mut self, now: tokio::time::Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.at).as_secs_f64();
        self.at = now;
        self.tokens = (self.tokens + elapsed * Self::RATE).min(Self::BURST);
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}
/// Unauthenticated connections (QUIC handshake through Welcome) may take at
/// most this long in total.
const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(10);
/// Connections still joining, across all sources.
const MAX_HANDSHAKES: usize = 64;
/// Connections still joining from one address. Enough for a LAN party behind
/// one router joining at once, too few for one source to fill the host.
const MAX_HANDSHAKES_PER_ADDRESS: usize = 8;
/// Package download connections, in total and from one address.
const MAX_DOWNLOADS: usize = 16;
const MAX_DOWNLOADS_PER_ADDRESS: usize = 2;
/// A global and a per-address bound on one kind of connection: those that
/// have not joined yet, or package downloads. Joined players are bounded by
/// the player limit.
#[derive(Clone)]
struct HandshakeGate {
    pending: Arc<Mutex<BTreeMap<IpAddr, usize>>>,
    total_limit: usize,
    address_limit: usize,
}
impl Default for HandshakeGate {
    fn default() -> Self {
        Self::new(MAX_HANDSHAKES, MAX_HANDSHAKES_PER_ADDRESS)
    }
}
/// One pending connection's share of the gate, returned when dropped.
struct HandshakeSlot {
    gate: HandshakeGate,
    address: IpAddr,
}
impl HandshakeGate {
    fn new(total_limit: usize, address_limit: usize) -> Self {
        Self {
            pending: Arc::default(),
            total_limit,
            address_limit,
        }
    }
    fn admit(&self, address: IpAddr) -> Option<HandshakeSlot> {
        let mut pending = self.pending.lock().ok()?;
        let total: usize = pending.values().sum();
        let from = pending.entry(address).or_default();
        if total >= self.total_limit || *from >= self.address_limit {
            if *from == 0 {
                pending.remove(&address);
            }
            return None;
        }
        *from += 1;
        Some(HandshakeSlot {
            gate: self.clone(),
            address,
        })
    }
    fn total(&self) -> usize {
        self.pending.lock().map_or(0, |p| p.values().sum())
    }
}
impl Drop for HandshakeSlot {
    fn drop(&mut self) {
        if let Ok(mut pending) = self.gate.pending.lock()
            && let Some(count) = pending.get_mut(&self.address)
        {
            *count -= 1;
            if *count == 0 {
                pending.remove(&self.address);
            }
        }
    }
}
/// A peer that stops reading for 10 s is gone.
async fn write_timed(send: &mut quinn::SendStream, bytes: &[u8]) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(10), codec::write_frame(send, bytes)).await?
}
fn verify_identity(
    hello: &Hello,
    nonce: &[u8; 32],
    server_fingerprint: &[u8; 32],
    require_identity: bool,
) -> Result<Option<Principal>> {
    hello.validate_bounds()?;
    let Some(proof) = &hello.identity else {
        ensure!(!require_identity, "This server requires persistent client identity proof");
        return Ok(None);
    };
    let transcript = identity_transcript(hello, nonce, server_fingerprint)?;
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, &proof.public_key)
        .verify(&transcript, &proof.signature)
        .map_err(|_| anyhow::anyhow!("Client identity proof failed"))?;
    let principal = Principal(Sha256::digest(proof.public_key).into());
    ensure!(principal.0 != [0; 32], "Invalid client principal");
    Ok(Some(principal))
}
fn token_key(token: &ResumeToken) -> [u8; 32] {
    Sha256::digest(token.0).into()
}
/// A server-issued resume capability. The host bit stays bound to the ticket
/// so a reconnect cannot claim a role through its Hello payload.
#[derive(Clone, Copy)]
struct Ticket {
    owner: OwnerId,
    host: bool,
    principal: Option<Principal>,
    issued: u64,
}
/// Bounded resume tickets. A full table forgets the least recently issued
/// ticket of a disconnected player (who then joins fresh) instead of
/// refusing every new player for the rest of the host's life.
#[derive(Default)]
struct Tickets {
    entries: BTreeMap<[u8; 32], Ticket>,
    issued: u64,
}
impl Tickets {
    const CAPACITY: usize = 4096;
    fn get(&self, key: &[u8; 32]) -> Option<Ticket> {
        self.entries.get(key).copied()
    }
    fn insert(
        &mut self,
        key: [u8; 32],
        mut ticket: Ticket,
        connected: impl Fn(OwnerId) -> bool,
    ) -> Result<()> {
        if !self.entries.contains_key(&key) && self.entries.len() >= Self::CAPACITY {
            let stale = self
                .entries
                .iter()
                .filter(|(_, t)| !connected(t.owner))
                .min_by_key(|(_, t)| t.issued)
                .map(|(key, _)| *key)
                .context("Server identity capacity reached")?;
            self.entries.remove(&stale);
        }
        self.issued += 1;
        ticket.issued = self.issued;
        self.entries.insert(key, ticket);
        Ok(())
    }
}
/// Send each client its view of package state when it changed: keys visible
/// to everyone plus its own owner-visible keys, never another player's.
fn send_package_views(
    session: &Session,
    peers: &BTreeMap<OwnerId, Peer>,
    sent: &mut BTreeMap<OwnerId, bri_sim::session::PackageStateView>,
) {
    for (owner, peer) in peers {
        let view = session.package_state_for(*owner);
        if sent.get(owner) != Some(&view) {
            peer.send_message(Kind::Package, &Message::PackageState(view.clone()));
            sent.insert(*owner, view);
        }
    }
}
/// Encode each state item once, then pack what each peer should get into as
/// few datagrams as fit.
fn send_state(
    peers: &BTreeMap<OwnerId, Peer>,
    traffic: &Traffic,
    items: Vec<(Datagram, crate::stream::Audience)>,
) {
    let encoded: Vec<_> = items
        .into_iter()
        .filter_map(|(item, audience)| {
            let kind = match item {
                Datagram::Pose(_) | Datagram::Remote(_) => Kind::Pose,
                Datagram::Vehicle(_) => Kind::Vehicle,
                Datagram::Orb(_) => Kind::Orb,
            };
            match codec::encode_datagram_item(&item) {
                Ok(bytes) => Some((kind, audience, bytes)),
                Err(error) => {
                    eprintln!("Server dropped a state datagram: {error:#}");
                    None
                }
            }
        })
        .collect();
    for (owner, peer) in peers {
        let mine = encoded.iter().filter(|(_, audience, _)| audience.includes(*owner));
        for (kind, _, bytes) in mine.clone() {
            traffic.add(*kind, bytes.len(), 1);
        }
        for datagram in codec::pack_datagrams(mine.map(|(_, _, bytes)| bytes.as_slice())) {
            let _ = peer.connection.send_datagram(datagram.into());
        }
    }
}
fn broadcast_admin_snapshots(session: &Session, peers: &BTreeMap<OwnerId, Peer>) {
    for (owner, peer) in peers {
        peer.bulk
            .store(session.is_administrator(*owner), Ordering::Relaxed);
        match session.admin_state(*owner) {
            Ok(snapshot) => peer.send_message(Kind::Admin, &Message::AdminSnapshot(snapshot)),
            Err(error) => {
                eprintln!("Server could not build an admin snapshot: {error:#}");
                peer.connection.close(2_u32.into(), b"Administration state unavailable");
            }
        }
    }
}
/// Let go the players the session asked to disconnect (kicks, bans, a map
/// that could not place them), telling each why.
fn close_admin_disconnects(session: &mut Session, peers: &mut BTreeMap<OwnerId, Peer>) {
    for target in session.take_admin_disconnects() {
        let message = session.take_admin_disconnect_message(target);
        if let Some(target_peer) = peers.remove(&target) {
            // The close frame must fit one packet; messages stay short.
            target_peer.connection.close(
                0_u32.into(),
                &message.as_bytes()[..message.floor_char_boundary(400)],
            );
            let _ = session.disconnect(target);
        }
    }
}

/// The text a panic was raised with.
fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".into())
}

/// A bug (a panic) in one request, join or tick costs that request, join or
/// tick, not the game for everyone: the host answers the request with an
/// error, logs it (the crash hook also writes a report with its backtrace)
/// and keeps serving. A host that keeps panicking is broken, not unlucky, so
/// past [`PanicFuse::LIMIT`] faults in [`PanicFuse::WINDOW`] it stops with
/// the real error, which saves the world on the way out.
#[derive(Default)]
struct PanicFuse {
    recent: std::collections::VecDeque<std::time::Instant>,
    total: u64,
}
impl PanicFuse {
    const LIMIT: usize = 8;
    const WINDOW: Duration = Duration::from_secs(60);
    /// `Ok(Ok(value))` normally, `Ok(Err(fault))` when `work` panicked, and
    /// `Err` once the fuse has blown.
    fn guard<T>(&mut self, what: &str, work: impl FnOnce() -> T) -> Result<std::result::Result<T, String>> {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
            Ok(value) => Ok(Ok(value)),
            Err(panic) => {
                let message = panic_message(&*panic);
                let now = std::time::Instant::now();
                self.total += 1;
                self.recent.push_back(now);
                while self
                    .recent
                    .front()
                    .is_some_and(|at| now.duration_since(*at) > Self::WINDOW)
                {
                    self.recent.pop_front();
                }
                eprintln!("Host fault {} in {what}: {message}", self.total);
                anyhow::ensure!(
                    self.recent.len() <= Self::LIMIT,
                    "The host kept failing ({} faults in a minute); last, in {what}: {message}",
                    self.recent.len()
                );
                Ok(Err(format!("The host hit an internal error in {what}; it was logged")))
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run(
    players: Arc<std::sync::atomic::AtomicU32>,
    perf: Arc<Mutex<ServerPerf>>,
    traffic: Arc<Traffic>,
    listing: Arc<std::sync::Mutex<Listing>>,
    endpoint: Endpoint,
    mut session: Session,
    options: ServerOptions,
    max_players: usize,
    host_key: [u8; 32],
    server_fingerprint: [u8; 32],
    require_identity: bool,
    mut admin_store: Option<AdminStore>,
    mut stop: oneshot::Receiver<()>,
) -> Result<ServerReport> {
    let (events, mut incoming) = mpsc::channel(256);
    let handshakes = HandshakeGate::default();
    let downloads = HandshakeGate::new(MAX_DOWNLOADS, MAX_DOWNLOADS_PER_ADDRESS);
    let shelf = options.packages.clone();
    let bulk_budget = Arc::new(Semaphore::new(codec::BULK_REQUEST_BUDGET));
    let mut tasks = tokio::task::JoinSet::new();
    let mut peers = BTreeMap::<OwnerId, Peer>::new();
    // Resume tokens are server-issued capabilities. Retain the host bit bound to
    // the ticket so a reconnect cannot claim a role through its Hello payload.
    let mut tickets = Tickets::default();
    let mut cursor = 0_u64;
    let mut names = BTreeMap::new();
    let mut avatars = BTreeMap::new();
    let mut tools = BTreeMap::new();
    let mut weapons = crate::stream::WeaponStream::default();
    weapons.reset(session.weapon_view(), session.simulation().state().tick, session.projectile_falls());
    let mut palette = session.simulation().state().palette.clone();
    let mut vitals = BTreeMap::new();
    let mut entities: BTreeMap<u64, _> = session.package_entities().into_iter().map(|e| (e.id, e)).collect();
    // Entities players who joined since the last update were handed.
    let mut joined_entities: Vec<Vec<bri_sim::session::EntityInfo>> = Vec::new();
    // What each client last received of package state (per viewer).
    let mut package_views: BTreeMap<OwnerId, bri_sim::session::PackageStateView> = BTreeMap::new();
    let mut minigames = Vec::new();
    let mut vehicles = Vec::new();
    let mut time_scale = session.time_scale();
    let mut broken_shapes = session.broken_shapes();
    let mut last_chat = 0;
    let mut state_stream = crate::stream::StateStream::default();
    let mut sent_dropped_cues = session.dropped_cues();
    let mut step_errors = 0_u64;
    // Event explosions/projectiles refused over the per-tick limits, logged
    // at most every ten seconds so a runaway loop cannot flood the log.
    let mut event_overload = (0_u64, None::<std::time::Instant>);
    let mut spawn_points = options.spawn_points.clone();
    let (map_tx, mut map_rx) = mpsc::channel::<(OwnerId, Result<Session>)>(1);
    let mut joins = 0;
    let mut resumes = 0;
    let mut commands = 0;
    let mut rejected = 0;
    let autosave = options.autosave.clone();
    let mut autosave_timer = tokio::time::interval(
        autosave.as_ref().map_or(Duration::from_secs(3600), |a| a.every.max(Duration::from_secs(1))),
    );
    autosave_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    autosave_timer.reset();
    let mut autosaving: Option<tokio::task::JoinHandle<Result<()>>> = None;
    let mut autosaves = 0_u64;
    let mut autosave_failures = 0_u64;
    let mut ticker = tokio::time::interval(Duration::from_secs_f64(1.0 / 120.0));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut clock = crate::tick_clock::TickClock::default();
    let mut previous = std::time::Instant::now();
    let mut perf_window = PerfWindow::default();
    let mut fuse = PanicFuse::default();
    let outcome:Result<()>=async {loop {tokio::select!{
        _=&mut stop=>break,
        accepted=endpoint.accept()=>{
            if let Some(accepted)=accepted {
                // Under load, make a source prove its address (a stateless
                // Retry round trip) before it may hold a pending slot, so
                // spoofed addresses cannot fill the per-address bounds.
                if !accepted.remote_address_validated() && accepted.may_retry() && handshakes.total()>=MAX_HANDSHAKES/2 {let _=accepted.retry();}
                else if let Some(slot)=handshakes.admit(accepted.remote_address().ip()){let events=events.clone();let bulk_budget=bulk_budget.clone();let downloads=(shelf.clone(),downloads.clone());let listing=current_listing(&listing,&players);tasks.spawn(async move{let deadline=tokio::time::Instant::now()+HANDSHAKE_DEADLINE;if let Ok(Ok(connection))=tokio::time::timeout_at(deadline,accepted).await {let _=connection_task(connection,events,bulk_budget,slot,deadline,server_fingerprint,require_identity,downloads,listing).await;}});}
                else{accepted.refuse();}
            }
        },
        Some(_)=tasks.join_next(),if !tasks.is_empty()=>{},
        _=autosave_timer.tick(),if autosave.is_some()=>{
            // One save in flight; a slow disk skips intervals, never queues them.
            if let Some(done)=autosaving.take_if(|task|task.is_finished()) {
                match done.await {Ok(Ok(()))=>autosaves+=1,Ok(Err(error))=>{autosave_failures+=1;eprintln!("Autosave failed: {error:#}");},Err(error)=>{autosave_failures+=1;eprintln!("Autosave failed: {error}");}}
            }
            if autosaving.is_none() && let Some(autosave)=&autosave {
                let world=session.saved_world();let save=autosave.save.clone();
                autosaving=Some(tokio::task::spawn_blocking(move||save(&world)));
            }
        },
        Some((admin,loaded))=map_rx.recv()=>{
            match loaded {
                Ok(new)=>{
                    // Every end of a world saves it, a map change included.
                    if let Some(autosave)=&autosave {
                        if let Some(task)=autosaving.take() {
                            match task.await {Ok(Ok(()))=>autosaves+=1,Ok(Err(error))=>{autosave_failures+=1;eprintln!("Autosave failed: {error:#}");},Err(error)=>{autosave_failures+=1;eprintln!("Autosave failed: {error}");}}
                        }
                        let world=session.saved_world();let save=autosave.save.clone();
                        match tokio::task::spawn_blocking(move||save(&world)).await {Ok(Ok(()))=>autosaves+=1,Ok(Err(error))=>{autosave_failures+=1;eprintln!("Autosave before map change failed: {error:#}");},Err(error)=>{autosave_failures+=1;eprintln!("Autosave before map change failed: {error}");}}
                    }
                    let old=std::mem::replace(&mut session,new);
                    session.adopt(old,admin)?;
                    // Players the new map could not place are let go with the reason.
                    close_admin_disconnects(&mut session,&mut peers);
                    if let Ok(mut listing)=listing.lock(){listing.map=session.simulation().state().map_id.clone();}
                    spawn_points=session.spawn_points().to_vec();
                    names=session.names();avatars=session.avatars();tools=session.tool_inventories();weapons.reset(session.weapon_view(),session.simulation().state().tick,session.projectile_falls());
                    palette=session.simulation().state().palette.clone();vitals=session.vitals();minigames=session.minigame_views();vehicles=session.vehicle_infos();broken_shapes=session.broken_shapes();entities=session.package_entities().into_iter().map(|e|(e.id,e)).collect();joined_entities.clear();
                    let (checkpoint,bricks)=Checkpoint::from_session(&session,cursor);
                    let transfer=encode_transfer(WorldTransfer{head:Message::MapChanged(checkpoint),bricks},traffic.clone(),peers.len());
                    for peer in peers.values(){peer.send(transfer.clone());}
                    package_views.clear();send_package_views(&session,&peers,&mut package_views);
                    broadcast_admin_snapshots(&session,&peers);
                }
                Err(error)=>session.map_change_failed(admin,&format!("{error:#}")),
            }
        },
        Some(event)=incoming.recv()=>{match event {
            Event::Join{hello,principal,connection,out,bulk,answer}=>{
                let join:Result<OwnerId>= match fuse.guard("a join",||(||{
                    ensure!(hello.version==VERSION,"Incompatible protocol version");let differences=check_packages(&options.environment,&hello.packages,hello.accept_differences)?;
                    ensure!(peers.len()<max_players,"Server is full");
                    let supplied_host=if let Some(host)=&hello.host {ensure!(token_key(host)==host_key,"Invalid host credential");true}else{false};
                    let (owner,token)=if let Some(token)=hello.resume {
                        let Ticket{owner,host:ticket_host,principal:ticket_principal,..}=tickets.get(&token_key(&token)).context("Invalid resume credential")?;
                        ensure!(!peers.contains_key(&owner),"Owner is still connected");
                        ensure!(ticket_principal==principal,"Resume identity does not match authenticated ticket");
                        let administrator=ticket_host || supplied_host;
                        let mut error=None;let mut found=false;for spawn in &spawn_points {match session.resume_verified(owner,*spawn,administrator,principal){Ok(())=>{found=true;break},Err(e)=>error=Some(e)}}ensure!(found,"{}",error.context("No spawn points")?);
                        tickets.insert(token_key(&token),Ticket{owner,host:administrator,principal,issued:0},|o|peers.contains_key(&o))?;
                        resumes+=1;(owner,token)
                    }else{
                        let mut bytes=[0;32];getrandom::fill(&mut bytes).map_err(|e|anyhow::anyhow!("OS randomness failed: {e}"))?;let token=ResumeToken(bytes);
                        let administrator=supplied_host;
                        let mut owner=None;let mut error=None;for spawn in &spawn_points {match session.join_verified(hello.name.clone(),*spawn,administrator,principal){Ok(id)=>{owner=Some(id);break},Err(e)=>error=Some(e)}}
                        let owner=owner.ok_or_else(||error.unwrap_or_else(||anyhow::anyhow!("No spawn points")))?;
                        if let Err(error)=tickets.insert(token_key(&token),Ticket{owner,host:administrator,principal,issued:0},|o|peers.contains_key(&o)){let _=session.disconnect(owner);return Err(error)}
                        joins+=1;(owner,token)
                    };
                    if !differences.unavailable.is_empty(){session.private_chat(owner,crate::client::unavailable_notice(&differences.unavailable));}
                    if !differences.cosmetic.is_empty(){session.private_chat(owner,format!("Some presentation packages differ from the server's, so things may look or sound different: {}",bri_package::environment::describe(&differences.cosmetic)));}
                    // O(1) on the loop; the world is chunked and encoded off it.
                    let (mut checkpoint,bricks)=Checkpoint::from_session(&session,cursor);
                    let view=session.package_state_for(owner);checkpoint.package_state=view.clone();
                    // The next update brings the joiner in line with everyone else.
                    weapons.joined(&checkpoint.weapons);joined_entities.push(checkpoint.entities.clone());
                    let welcome=encode_transfer(WorldTransfer{head:Message::Welcome{owner,administrator:session.is_administrator(owner),resume:token,checkpoint},bricks},traffic.clone(),1);
                    if out.try_send(welcome).is_err(){let _=session.disconnect(owner);anyhow::bail!("Join writer unavailable");}
                    bulk.store(session.is_administrator(owner),Ordering::Relaxed);
                    peers.insert(owner,Peer{generation:connection.stable_id(),connection,out,traffic:traffic.clone(),bulk});package_views.insert(owner,view);Ok(owner)
                })())? {Ok(join)=>join,Err(fault)=>Err(anyhow::anyhow!("{fault}"))};
                if join.is_err(){rejected+=1;}else{broadcast_admin_snapshots(&session,&peers);}let _=answer.send(join.map_err(|e|match e.downcast::<crate::client::PackagesDiffer>(){Ok(d)=>Message::PackagesDiffer(d.0),Err(e)=>Message::Rejected(e.to_string())}));
            },
            Event::Lost{owner,generation}=>{if peers.get(&owner).is_some_and(|p|p.generation==generation){peers.remove(&owner);package_views.remove(&owner);let _=session.disconnect(owner);broadcast_admin_snapshots(&session,&peers);}},
            Event::Command{owner,generation,request,_body_permit}=>{
                if let Some(peer)=peers.get(&owner).filter(|p|p.generation==generation){
                    commands+=1;
                    let old_admin_revision=session.admin_revision();
                    let result=match fuse.guard("a player's request",||session.command_with_aim_and_admin_persistence(owner,request.sequence,request.command,request.aim,|state|match admin_store.as_mut(){Some(store)=>store.persist(state),None=>anyhow::bail!("Persistent administration storage is not configured")}))? {Ok(result)=>result,Err(fault)=>Err(anyhow::anyhow!("{fault}"))};
                    if result.is_err(){rejected+=1;}
                    match codec::encode(&Message::Reply{sequence:request.sequence,result:result.map_err(|e|bri_sim::session::Rejection::from_error(&e))}) {
                        Ok(bytes)=>{traffic.add(Kind::Reply,bytes.len(),1);peer.send(Frame::Ready(Arc::new(bytes)))},
                        Err(error)=>peer.send_message(Kind::Reply,&Message::Reply{sequence:request.sequence,result:Err(bri_sim::session::Rejection::message(format!("Could not transfer reply: {error}")))}),
                    }
                    close_admin_disconnects(&mut session,&mut peers);
                    if old_admin_revision!=session.admin_revision(){broadcast_admin_snapshots(&session,&peers);}
                    if let Some((admin,map))=session.take_map_change(){
                        match options.map_loader.clone() {
                            Some(loader)=>{let tx=map_tx.clone();tokio::task::spawn_blocking(move||{
                                // A loader that panics still answers the administrator.
                                let loaded=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||loader(&map))).unwrap_or_else(|panic|Err(anyhow::anyhow!("Loading the map failed: {}",panic_message(&*panic))));
                                let _=tx.blocking_send((admin,loaded));});}
                            None=>session.map_change_failed(admin,"This host cannot change maps"),
                        }
                    }
                    if admin_store.as_ref().is_some_and(AdminStore::poisoned) {
                        anyhow::bail!("Admin store commit durability is uncertain; host stopped without publishing the request")
                    }
                }
            },
            Event::Move{owner,generation,movement}=>{if peers.get(&owner).is_some_and(|p|p.generation==generation){for (sequence,input) in movement.sequenced(){let _=session.movement(owner,sequence,input);}if let Some(camera)=movement.camera{let _=session.camera_report(owner,camera);}}},
        }},
        _=ticker.tick()=>{
            players.store(peers.len() as u32,std::sync::atomic::Ordering::Relaxed);
            let now=std::time::Instant::now();
            let steps=clock.advance(now.duration_since(previous).mul_f32(session.time_scale()));previous=now;
            for _ in 0..steps {
            let started=std::time::Instant::now();
            let stepped=match fuse.guard("a server tick",||session.step())? {Ok(stepped)=>stepped,Err(fault)=>Err(anyhow::anyhow!("{fault}"))};
            perf_window.step(started.elapsed());
            // A failing gameplay adapter must not stop the host for everyone.
            if let Err(error)=stepped{step_errors+=1;if step_errors<=16||step_errors.is_power_of_two(){eprintln!("Server step error ({step_errors}): {error:#}");}}
            let tick=session.simulation().state().tick;
            if tick.is_multiple_of(POSE_INTERVAL) {
                send_state(&peers,&traffic,state_stream.interval(tick,poses(&session),session.vehicle_poses(),session.camera_orbs()));
            }
            if tick.is_multiple_of(UPDATE_INTERVAL) {
                let mut bricks=BTreeMap::new();for id in session.take_dirty(){bricks.insert(id,session.simulation().state().bricks.get(&id).map(public_brick));}
                let changed_avatars=crate::stream::changed_entries(&mut avatars,session.avatars());
                let changed_tools=crate::stream::changed_entries(&mut tools,session.tool_inventories());
                let changed_weapons=weapons.delta(&session.weapon_view(),tick);
                let current_palette=&session.simulation().state().palette;let changed_palette=if &palette!=current_palette{palette=current_palette.clone();Some(palette.clone())}else{None};
                let current_names=session.names();let changed_names=if names!=current_names{names=current_names;Some(names.clone())}else{None};
                let changed_vitals=crate::stream::changed_entries(&mut vitals,session.vitals());
                let changed_entities=EntityDelta::between_joined(&mut entities,&std::mem::take(&mut joined_entities),session.package_entities());
                let current_minigames=session.minigame_views();let changed_minigames=if minigames!=current_minigames{minigames=current_minigames;Some(minigames.clone())}else{None};
                let changed_time_scale=(time_scale!=session.time_scale()).then(||{time_scale=session.time_scale();time_scale});let current_vehicles=session.vehicle_infos();let changed_vehicles=if vehicles!=current_vehicles{vehicles=current_vehicles;Some(vehicles.clone())}else{None};
                let current_broken=session.broken_shapes();let changed_broken=if broken_shapes!=current_broken{broken_shapes=current_broken;Some(broken_shapes.clone())}else{None};
                let chat:Vec<_>=session.chat().into_iter().filter(|c|c.id>last_chat).collect();if let Some(line)=chat.last(){last_chat=line.id;}
                let next=cursor.checked_add(1).context("Replication sequence exhausted")?;
                let cues=session.take_cues();let dropped_cues=session.dropped_cues();
                let delta=Delta{base:cursor,cursor:next,tick,bricks,names:changed_names,avatars:changed_avatars,tools:changed_tools,weapons:changed_weapons,palette:changed_palette,chat,cues,dropped_cues,vitals:changed_vitals,minigames:changed_minigames,vehicles:changed_vehicles,time_scale:changed_time_scale,broken_shapes:changed_broken,entities:changed_entities};
                // An update with nothing in it only moves the clients' clock.
                // Clients coast projectiles on each update's tick, so they keep 20 Hz.
                if !delta.is_empty() || dropped_cues!=sent_dropped_cues || weapons.in_flight() || tick.is_multiple_of(HEARTBEAT_INTERVAL) {
                    sent_dropped_cues=dropped_cues;
                    broadcast(peers.values(),Kind::Update,&Message::Update(delta));cursor=next;
                }
                send_package_views(&session,&peers,&mut package_views);
                for (owner,notice) in session.take_private_notices(){if let Some(peer)=peers.get(&owner){peer.send_message(Kind::Notice,&Message::Notice(notice));}}
            }
            }
            event_overload.0+=session.take_event_overload();
            if event_overload.0>0 && event_overload.1.is_none_or(|at|now.duration_since(at)>=Duration::from_secs(10)) {
                eprintln!("Events: {} explosions/projectiles over the per-tick limit were dropped",event_overload.0);
                event_overload=(0,Some(now));
            }
            if perf_window.started.is_none_or(|at|now.duration_since(at)>=PerfWindow::LENGTH)
                && let Some(summary)=perf_window.finish(now,session.take_package_script_time(),peers.len() as u32)
            {
                *perf.lock().unwrap_or_else(|e|e.into_inner())=summary;
            }
        },
    }}Ok(())}.await;
    endpoint.close(0_u32.into(), b"Server shutdown");
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    if let Some(task) = autosaving.take() {
        match task.await {
            Ok(Ok(())) => autosaves += 1,
            _ => autosave_failures += 1,
        }
    }
    if let (Err(error), Some(autosave)) = (&outcome, &autosave) {
        // The report (and its world) is lost with the error; keep the world.
        eprintln!("Host stopped with an error ({error:#}); saving its world");
        let world = session.saved_world();
        let save = autosave.save.clone();
        if let Err(save_error) = tokio::task::spawn_blocking(move || save(&world)).await? {
            eprintln!("Final autosave failed: {save_error:#}");
        }
    }
    outcome?;
    Ok(ServerReport {
        step_errors,
        weapon_adapter_gaps: session.weapon_adapter_gaps().clone(),
        ticks: session.simulation().state().tick,
        dropped_ticks: clock.dropped,
        dropped_cues: session.dropped_cues(),
        joins,
        resumes,
        commands,
        rejected,
        final_world: {
            let world = session.simulation().state();
            PublicWorld {
                name: world.name.clone(),
                map_id: world.map_id.clone(),
                palette: world.palette.clone(),
                bricks: public_bricks(&world.bricks),
            }
        },
        autosaves,
        autosave_failures,
        native_world: session.saved_world(),
        notices: session.take_notices(),
        packages: session.package_save(),
        package_diagnostics: session.package_diagnostics(),
        package_stats: session.package_stats(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_panicking_request_is_answered_and_the_host_keeps_going_until_the_fuse_blows() {
        let mut fuse = PanicFuse::default();
        assert_eq!(fuse.guard("a tick", || 7).unwrap(), Ok(7));
        for _ in 0..PanicFuse::LIMIT {
            let fault = fuse
                .guard("a player's request", || -> u32 { panic!("bug in a handler") })
                .expect("one fault does not stop the host");
            assert_eq!(
                fault,
                Err("The host hit an internal error in a player's request; it was logged".into())
            );
        }
        let blown = fuse
            .guard("a player's request", || -> u32 { panic!("bug in a handler") })
            .expect_err("a host that keeps failing stops");
        assert!(format!("{blown:#}").contains("bug in a handler"), "{blown:#}");
    }
    #[test]
    fn host_certificate_is_one_file_kept_across_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let first = HostCertificate::load_or_create(dir.path()).unwrap();
        let again = HostCertificate::load_or_create(dir.path()).unwrap();
        assert_eq!((first.der.clone(), first.key.clone()), (again.der, again.key));
        assert!(dir.path().join("host-identity.bin").is_file());
        // A damaged file is reported and left in place, never silently replaced.
        let path = dir.path().join("host-identity.bin");
        let mut bytes = std::fs::read(&path).unwrap();
        bytes.truncate(bytes.len() - 3);
        std::fs::write(&path, &bytes).unwrap();
        let error = HostCertificate::load_or_create(dir.path()).err().unwrap();
        assert!(format!("{error:#}").contains("damaged"), "{error:#}");
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    fn ticket(owner: OwnerId) -> Ticket {
        Ticket {
            owner,
            host: false,
            principal: None,
            issued: 0,
        }
    }
    #[test]
    fn perf_window_summarises_a_second_of_steps() {
        let t0 = std::time::Instant::now();
        let mut w = PerfWindow::default();
        assert!(w.finish(t0, BTreeMap::new(), 0).is_none());
        for ms in [2, 4, 6, 8] {
            w.step(Duration::from_millis(ms));
        }
        let half = t0 + Duration::from_millis(500);
        assert!(w.finish(half, BTreeMap::new(), 0).is_none());
        let script = BTreeMap::from([
            ("quiet".to_string(), Duration::from_millis(1)),
            ("busy".to_string(), Duration::from_millis(8)),
        ]);
        let p = w.finish(t0 + Duration::from_secs(2), script, 3).unwrap();
        assert_eq!(p.ticks_per_second, 2.0);
        assert_eq!(p.tick_ms_mean, 5.0);
        assert_eq!(p.tick_ms_max, 8.0);
        assert_eq!(p.players, 3);
        assert_eq!(p.script_ms, vec![("busy".into(), 2.0), ("quiet".into(), 0.25)]);
        // The next window starts empty.
        assert_eq!(w.steps, 0);
    }
    #[test]
    fn full_ticket_table_forgets_the_oldest_disconnected_player() {
        let mut tickets = Tickets::default();
        for n in 0..Tickets::CAPACITY as u64 {
            let mut key = [0; 32];
            key[..8].copy_from_slice(&n.to_le_bytes());
            tickets.insert(key, ticket(n + 1), |_| false).unwrap();
        }
        // Owner 1 holds the oldest ticket but is still connected.
        tickets.insert([0xff; 32], ticket(9999), |o| o == 1).unwrap();
        assert_eq!(tickets.entries.len(), Tickets::CAPACITY);
        assert!(tickets.get(&[0; 32]).is_some(), "connected owner kept");
        let mut second = [0; 32];
        second[0] = 1;
        assert!(tickets.get(&second).is_none(), "oldest disconnected evicted");
        assert_eq!(tickets.get(&[0xff; 32]).unwrap().owner, 9999);
        // Refreshing an existing ticket never evicts.
        tickets.insert([0xff; 32], ticket(9999), |_| true).unwrap();
        assert!(tickets.insert([0xee; 32], ticket(1), |_| true).is_err());
    }
    #[test]
    fn handshake_gate_bounds_each_address_and_the_total() {
        let gate = HandshakeGate::default();
        let one: IpAddr = "10.0.0.1".parse().unwrap();
        let held: Vec<_> = (0..MAX_HANDSHAKES_PER_ADDRESS)
            .map(|_| gate.admit(one).unwrap())
            .collect();
        assert!(gate.admit(one).is_none(), "per-address bound");
        let mut others = Vec::new();
        for n in 0..=255_u8 {
            match gate.admit(IpAddr::from([10, 0, 1, n])) {
                Some(slot) => others.push(slot),
                None => break,
            }
        }
        assert_eq!(held.len() + others.len(), MAX_HANDSHAKES, "total bound");
        drop(held);
        assert!(gate.admit(one).is_some(), "slots return when dropped");
        drop(others);
        assert_eq!(gate.total(), 0);
    }
    #[test]
    fn movement_allowance_bounds_a_flood_but_not_a_catch_up_burst() {
        let start = tokio::time::Instant::now();
        let mut allowance = MovementAllowance::new(start);
        let burst = (0..1000).filter(|_| allowance.take(start)).count();
        assert_eq!(burst, MovementAllowance::BURST as usize);
        let later = start + Duration::from_millis(500);
        let refill = (0..1000).filter(|_| allowance.take(later)).count();
        assert_eq!(refill, MovementAllowance::BURST as usize, "capped refill");
        let tick = later + Duration::from_secs_f64(1.0 / 120.0);
        assert!(allowance.take(tick), "one datagram per tick always passes");
    }
}
