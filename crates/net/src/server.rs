use crate::{admin_store::AdminStore, codec, protocol::*};
use anyhow::{Context, Result, ensure};
use bri_admin::Principal;
use bri_sim::session::Session;
use bri_world::OwnerId;
use glam::Vec3;
use quinn::{Connection, Endpoint};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, net::SocketAddr, sync::Arc, time::Duration};
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
}
/// Refuse a join whose shared packages differ from the server's, naming
/// every differing package. Presentation-only differences are allowed and
/// returned, for the joining player to be told about.
fn check_packages(
    environment: &bri_package::environment::Environment,
    client: &[bri_package::environment::PackageRef],
) -> Result<Vec<bri_package::environment::Mismatch>> {
    let (blocking, cosmetic): (Vec<_>, Vec<_>) = environment
        .compare(client)
        .into_iter()
        .partition(|m| m.blocks_join());
    ensure!(
        blocking.is_empty(),
        "{}",
        bri_package::environment::refusal(&blocking)
    );
    Ok(cosmetic)
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
    discovery: Option<tokio::task::JoinHandle<()>>,
    router: Option<RouterPorts>,
    stop: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<Result<ServerReport>>,
}
#[derive(Debug, Serialize)]
pub struct ServerReport {
    pub weapon_adapter_gaps: BTreeMap<String, u64>,
    pub ticks: u64,
    pub dropped_ticks: u64,
    pub dropped_cues: u64,
    pub joins: u64,
    pub resumes: u64,
    pub commands: u64,
    pub rejected: u64,
    pub final_world: PublicWorld,
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
        let discovery = crate::discovery::DISCOVERY_PORT;
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
    /// Internet hosts: ask the router (UPnP) to forward the game and
    /// certificate ports for as long as this host runs. Each outcome is sent
    /// to `notify` as a line for the host player.
    pub fn open_router_ports(&mut self, notify: std::sync::mpsc::Sender<String>) {
        let port = self.address.port();
        let ports = vec![port, crate::discovery::DISCOVERY_PORT];
        let slot = Arc::new(std::sync::Mutex::new(None::<crate::upnp::PortMapping>));
        let held = slot.clone();
        let task = tokio::spawn(async move {
            let opened = tokio::task::spawn_blocking(move || crate::upnp::PortMapping::open(&ports)).await;
            let mapping = match opened {
                Ok(Ok(mapping)) => mapping,
                Ok(Err(error)) => {
                    let _ = notify.send(format!(
                        "Could not open router ports automatically ({error}). Friends outside your network need UDP {port} and {} forwarded to this PC.",
                        crate::discovery::DISCOVERY_PORT
                    ));
                    return;
                }
                Err(_) => return,
            };
            let _ = notify.send(match mapping.external_ip {
                Some(ip) if mapping.behind_another_router() => format!(
                    "Router ports opened, but your router's address {ip} is not public (another router or your provider sits in front). Friends outside probably cannot connect."
                ),
                Some(ip) => format!("Router ports opened. Friends can Connect to IP: {ip}:{port}"),
                None => format!("Router ports opened. Friends can Connect to IP with your public IP and port {port}."),
            });
            if let Ok(mut guard) = held.lock() {
                *guard = Some(mapping);
            }
            loop {
                tokio::time::sleep(crate::upnp::RENEW_EVERY).await;
                let held = held.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    if let Ok(mut guard) = held.lock()
                        && let Some(mapping) = guard.as_mut()
                    {
                        let _ = mapping.renew();
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
/// Router forwards held for a running internet host. Dropping this removes
/// them on a background thread so the caller never waits on the router.
struct RouterPorts {
    task: tokio::task::JoinHandle<()>,
    slot: Arc<std::sync::Mutex<Option<crate::upnp::PortMapping>>>,
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
fn encode_transfer(transfer: WorldTransfer) -> Frame {
    let (ready, frames) = watch::channel(None);
    tokio::task::spawn_blocking(move || {
        let encoded = transfer
            .encode()
            .map(Arc::from)
            .map_err(|error| format!("{error:#}"));
        let _ = ready.send(Some(encoded));
    });
    Frame::Pending(frames)
}
struct Peer {
    connection: Connection,
    out: mpsc::Sender<Frame>,
    generation: usize,
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
    fn send_message(&self, message: &Message) {
        match codec::encode(message) {
            Ok(bytes) => self.send(Frame::Ready(Arc::new(bytes))),
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
fn broadcast<'a>(peers: impl IntoIterator<Item = &'a Peer>, message: &Message) {
    match codec::encode(message) {
        Ok(bytes) => {
            let frame = Frame::Ready(Arc::new(bytes));
            for peer in peers {
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
        out: mpsc::Sender<Frame>,
        answer: oneshot::Sender<Result<OwnerId, String>>,
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
    let task = tokio::spawn(run(
        players.clone(),
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
        discovery: None,
        router: None,
        stop: Some(stop_tx),
        task,
    })
}
async fn connection_task(
    connection: Connection,
    events: mpsc::Sender<Event>,
    request_budget: Arc<Semaphore>,
    server_fingerprint: [u8; 32],
    require_identity: bool,
) -> Result<()> {
    let (mut send, mut receive) =
        tokio::time::timeout(Duration::from_secs(10), connection.accept_bi()).await??;
    let begin: JoinBegin = tokio::time::timeout(
        Duration::from_secs(10),
        codec::read_small_request(&mut receive),
    )
    .await??;
    if begin.version != VERSION {
        codec::write_frame(&mut send, &codec::encode(&Message::Rejected("Incompatible protocol version".into()))?).await?;
        send.finish()?;
        return Ok(());
    }
    let mut nonce = [0; 32];
    getrandom::fill(&mut nonce).map_err(|error| anyhow::anyhow!("OS randomness failed: {error}"))?;
    codec::write_frame(
        &mut send,
        &codec::encode(&Message::Challenge { nonce })?,
    )
    .await?;
    let hello: Hello = tokio::time::timeout(
        Duration::from_secs(10),
        codec::read_small_request(&mut receive),
    )
    .await??;
    let principal = match verify_identity(&hello, &nonce, &server_fingerprint, require_identity) {
        Ok(principal) => principal,
        Err(error) => {
            let bytes = codec::encode(&Message::Rejected(error.to_string()))?;
            let _ = codec::write_frame(&mut send, &bytes).await;
            let _ = send.finish();
            let _ = tokio::time::timeout(Duration::from_secs(1), send.stopped()).await;
            return Ok(());
        }
    };
    let (out, mut output) = mpsc::channel::<Frame>(32);
    let (answer, accepted) = oneshot::channel();
    events
        .send(Event::Join {
            hello,
            principal,
            connection: connection.clone(),
            out,
            answer,
        })
        .await?;
    let owner = match accepted.await? {
        Ok(owner) => owner,
        Err(reason) => {
            codec::write_frame(&mut send, &codec::encode(&Message::Rejected(reason))?).await?;
            send.finish()?;
            let _ = tokio::time::timeout(Duration::from_secs(1), send.stopped()).await;
            return Ok(());
        }
    };
    let generation = connection.stable_id();
    let write = async {
        while let Some(frame) = output.recv().await {
            match frame {
                Frame::Ready(bytes) => write_timed(&mut send, &bytes).await?,
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
            let (request, permit) =
                codec::read_budgeted_request(&mut receive, &request_budget).await?;
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
fn broadcast_admin_snapshots(session: &Session, peers: &BTreeMap<OwnerId, Peer>) {
    for (owner, peer) in peers {
        match session.admin_state(*owner) {
            Ok(snapshot) => peer.send_message(&Message::AdminSnapshot(snapshot)),
            Err(error) => {
                eprintln!("Server could not build an admin snapshot: {error:#}");
                peer.connection.close(2_u32.into(), b"Administration state unavailable");
            }
        }
    }
}
#[allow(clippy::too_many_arguments)]
async fn run(
    players: Arc<std::sync::atomic::AtomicU32>,
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
    let permits = Arc::new(Semaphore::new(80));
    let request_budget = Arc::new(Semaphore::new(codec::REQUEST_BODY_BUDGET));
    let mut tasks = tokio::task::JoinSet::new();
    let mut peers = BTreeMap::<OwnerId, Peer>::new();
    // Resume tokens are server-issued capabilities. Retain the host bit bound to
    // the ticket so a reconnect cannot claim a role through its Hello payload.
    let mut tickets = Tickets::default();
    let mut cursor = 0_u64;
    let mut names = BTreeMap::new();
    let mut avatars = BTreeMap::new();
    let mut tools = BTreeMap::new();
    let mut weapons = bri_sim::session::WeaponView::default();
    let mut palette = session.simulation().state().palette.clone();
    let mut vitals = BTreeMap::new();
    let mut entities = session.package_entities();
    let mut package_state = session.package_state();
    let mut minigames = Vec::new();
    let mut vehicles = Vec::new();
    let mut time_scale = session.time_scale();
    let mut broken_shapes = session.broken_shapes();
    let mut last_chat = 0;
    let mut step_errors = 0_u64;
    let mut spawn_points = options.spawn_points.clone();
    let (map_tx, mut map_rx) = mpsc::channel::<(OwnerId, Result<Session>)>(1);
    let mut joins = 0;
    let mut resumes = 0;
    let mut commands = 0;
    let mut rejected = 0;
    let mut ticker = tokio::time::interval(Duration::from_secs_f64(1.0 / 120.0));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut clock = crate::tick_clock::TickClock::default();
    let mut previous = std::time::Instant::now();
    let outcome:Result<()>=async {loop {tokio::select!{
        _=&mut stop=>break,
        accepted=endpoint.accept()=>{
            if let Some(accepted)=accepted {if let Ok(permit)=permits.clone().try_acquire_owned(){let events=events.clone();let request_budget=request_budget.clone();tasks.spawn(async move{let _permit=permit;if let Ok(Ok(connection))=tokio::time::timeout(Duration::from_secs(10),accepted).await {let _=connection_task(connection,events,request_budget,server_fingerprint,require_identity).await;}});}else{accepted.refuse();}}
        },
        Some(_)=tasks.join_next(),if !tasks.is_empty()=>{},
        Some((admin,loaded))=map_rx.recv()=>{
            match loaded {
                Ok(new)=>{
                    let old=std::mem::replace(&mut session,new);
                    session.adopt(old,admin)?;
                    spawn_points=session.spawn_points().to_vec();
                    names=session.names();avatars=session.avatars();tools=session.tool_inventories();weapons=session.weapon_view();
                    palette=session.simulation().state().palette.clone();vitals=session.vitals();minigames=session.minigame_views();vehicles=session.vehicle_infos();broken_shapes=session.broken_shapes();entities=session.package_entities();package_state=session.package_state();
                    let (checkpoint,bricks)=Checkpoint::from_session(&session,cursor);
                    let transfer=encode_transfer(WorldTransfer{head:Message::MapChanged(checkpoint),bricks});
                    for peer in peers.values(){peer.send(transfer.clone());}
                    broadcast_admin_snapshots(&session,&peers);
                }
                Err(error)=>session.map_change_failed(admin,&format!("{error:#}")),
            }
        },
        Some(event)=incoming.recv()=>{match event {
            Event::Join{hello,principal,connection,out,answer}=>{
                let join:Result<OwnerId>= (||{
                    ensure!(hello.version==VERSION,"Incompatible protocol version");let cosmetic=check_packages(&options.environment,&hello.packages)?;
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
                    if !cosmetic.is_empty(){session.private_chat(owner,format!("Some presentation packages differ from the server's, so things may look or sound different: {}",bri_package::environment::describe(&cosmetic)));}
                    // O(1) on the loop; the world is chunked and encoded off it.
                    let (checkpoint,bricks)=Checkpoint::from_session(&session,cursor);
                    let welcome=encode_transfer(WorldTransfer{head:Message::Welcome{owner,administrator:session.is_administrator(owner),resume:token,checkpoint},bricks});
                    if out.try_send(welcome).is_err(){let _=session.disconnect(owner);anyhow::bail!("Join writer unavailable");}
                    peers.insert(owner,Peer{generation:connection.stable_id(),connection,out});Ok(owner)
                })();
                if join.is_err(){rejected+=1;}else{broadcast_admin_snapshots(&session,&peers);}let _=answer.send(join.map_err(|e|e.to_string()));
            },
            Event::Lost{owner,generation}=>{if peers.get(&owner).is_some_and(|p|p.generation==generation){peers.remove(&owner);let _=session.disconnect(owner);broadcast_admin_snapshots(&session,&peers);}},
            Event::Command{owner,generation,request,_body_permit}=>{
                if let Some(peer)=peers.get(&owner).filter(|p|p.generation==generation){
                    commands+=1;
                    let old_admin_revision=session.admin_revision();
                    let result=session.command_with_aim_and_admin_persistence(owner,request.sequence,request.command,request.aim,|state|match admin_store.as_mut(){Some(store)=>store.persist(state),None=>anyhow::bail!("Persistent administration storage is not configured")});
                    if result.is_err(){rejected+=1;}
                    match codec::encode(&Message::Reply{sequence:request.sequence,result:result.map_err(|e|bri_sim::session::Rejection::from_error(&e))}) {
                        Ok(bytes)=>peer.send(Frame::Ready(Arc::new(bytes))),
                        Err(error)=>peer.send_message(&Message::Reply{sequence:request.sequence,result:Err(bri_sim::session::Rejection::message(format!("Could not transfer reply: {error}")))}),
                    }
                    for target in session.take_admin_disconnects(){
                        if let Some(target_peer)=peers.remove(&target){
                            target_peer.connection.close(0_u32.into(),b"Administration disconnect");
                            let _=session.disconnect(target);
                        }
                    }
                    if old_admin_revision!=session.admin_revision(){broadcast_admin_snapshots(&session,&peers);}
                    if let Some((admin,map))=session.take_map_change(){
                        match options.map_loader.clone() {
                            Some(loader)=>{let tx=map_tx.clone();tokio::task::spawn_blocking(move||{let _=tx.blocking_send((admin,loader(&map)));});}
                            None=>session.map_change_failed(admin,"This host cannot change maps"),
                        }
                    }
                    if admin_store.as_ref().is_some_and(AdminStore::poisoned) {
                        anyhow::bail!("Admin store commit durability is uncertain; host stopped without publishing the request")
                    }
                }
            },
            Event::Move{owner,generation,movement}=>{if peers.get(&owner).is_some_and(|p|p.generation==generation){for (sequence,input) in movement.sequenced(){let _=session.movement(owner,sequence,input);}}},
        }},
        _=ticker.tick()=>{
            players.store(peers.len() as u32,std::sync::atomic::Ordering::Relaxed);
            let now=std::time::Instant::now();
            let steps=clock.advance(now.duration_since(previous).mul_f32(session.time_scale()));previous=now;
            for _ in 0..steps {
            // A failing gameplay adapter must not stop the host for everyone.
            if let Err(error)=session.step(){step_errors+=1;if step_errors<=16||step_errors.is_power_of_two(){eprintln!("Server step error ({step_errors}): {error:#}");}}
            let tick=session.simulation().state().tick;
            if tick.is_multiple_of(POSE_INTERVAL) {
                let datagrams=poses(&session).into_iter().map(Datagram::Pose).chain(session.vehicle_poses().into_iter().map(Datagram::Vehicle));
                for datagram in datagrams {
                    let bytes:bytes::Bytes=match codec::encode_datagram(&datagram){Ok(bytes)=>bytes.into(),Err(error)=>{eprintln!("Server dropped a state datagram: {error:#}");continue}};
                    for peer in peers.values(){let _=peer.connection.send_datagram(bytes.clone());}
                }
            }
            if tick.is_multiple_of(6) {
                let mut bricks=BTreeMap::new();for id in session.take_dirty(){bricks.insert(id,session.simulation().state().bricks.get(&id).map(public_brick));}
                let current_avatars=session.avatars();let changed_avatars=if avatars!=current_avatars{avatars=current_avatars;Some(avatars.clone())}else{None};
                let current_tools=session.tool_inventories();let changed_tools=if tools!=current_tools{tools=current_tools;Some(tools.clone())}else{None};
                let current_weapons=session.weapon_view();let changed_weapons=if weapons!=current_weapons{weapons=current_weapons;Some(weapons.clone())}else{None};
                let current_palette=&session.simulation().state().palette;let changed_palette=if &palette!=current_palette{palette=current_palette.clone();Some(palette.clone())}else{None};
                let current_names=session.names();let changed_names=if names!=current_names{names=current_names;Some(names.clone())}else{None};
                let current_vitals=session.vitals();let changed_vitals=if vitals!=current_vitals{vitals=current_vitals;Some(vitals.clone())}else{None};
                let current_entities=session.package_entities();let changed_entities=if entities!=current_entities{entities=current_entities;Some(entities.clone())}else{None};
                let current_state=session.package_state();let changed_state=if package_state!=current_state{package_state=current_state;Some(package_state.clone())}else{None};
                let current_minigames=session.minigame_views();let changed_minigames=if minigames!=current_minigames{minigames=current_minigames;Some(minigames.clone())}else{None};
                let changed_time_scale=(time_scale!=session.time_scale()).then(||{time_scale=session.time_scale();time_scale});let current_vehicles=session.vehicle_infos();let changed_vehicles=if vehicles!=current_vehicles{vehicles=current_vehicles;Some(vehicles.clone())}else{None};
                let current_broken=session.broken_shapes();let changed_broken=if broken_shapes!=current_broken{broken_shapes=current_broken;Some(broken_shapes.clone())}else{None};
                let chat:Vec<_>=session.chat().into_iter().filter(|c|c.id>last_chat).collect();if let Some(line)=chat.last(){last_chat=line.id;}
                let next=cursor.checked_add(1).context("Replication sequence exhausted")?;
                let cues=session.take_cues();let dropped_cues=session.dropped_cues();
                broadcast(peers.values(),&Message::Update(Delta{base:cursor,cursor:next,tick,bricks,names:changed_names,avatars:changed_avatars,tools:changed_tools,weapons:changed_weapons,palette:changed_palette,chat,cues,dropped_cues,vitals:changed_vitals,minigames:changed_minigames,vehicles:changed_vehicles,time_scale:changed_time_scale,broken_shapes:changed_broken,entities:changed_entities,package_state:changed_state}));cursor=next;
                for (owner,notice) in session.take_private_notices(){if let Some(peer)=peers.get(&owner){peer.send_message(&Message::Notice(notice));}}
            }
            }
        },
    }}Ok(())}.await;
    endpoint.close(0_u32.into(), b"Server shutdown");
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    outcome?;
    Ok(ServerReport {
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
        native_world: session.simulation().state().clone(),
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
