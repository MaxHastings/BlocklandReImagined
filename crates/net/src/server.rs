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
use tokio::sync::{Semaphore, mpsc, oneshot};

pub struct ServerOptions {
    pub bind: SocketAddr,
    pub content_id: String,
    pub spawn_points: Vec<Vec3>,
    /// A persistent host identity lets joiners keep trusting this host
    /// across restarts. None generates a throwaway certificate.
    pub certificate: Option<HostCertificate>,
}
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
    /// Load `host-certificate.der` / `host-key.der` from a private state
    /// directory, creating them on first use.
    pub fn load_or_create(dir: &std::path::Path) -> Result<Self> {
        let cert_path = dir.join("host-certificate.der");
        let key_path = dir.join("host-key.der");
        if let (Ok(der), Ok(key)) = (std::fs::read(&cert_path), std::fs::read(&key_path))
            && !der.is_empty()
            && der.len() <= 16384
            && !key.is_empty()
            && key.len() <= 16384
        {
            return Ok(Self { der, key });
        }
        let identity = Self::generate()?;
        std::fs::create_dir_all(dir)?;
        std::fs::write(&key_path, &identity.key)?;
        std::fs::write(&cert_path, &identity.der)?;
        Ok(identity)
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
struct Peer {
    connection: Connection,
    out: mpsc::Sender<Arc<Vec<u8>>>,
    generation: usize,
}
enum Event {
    Join {
        hello: Hello,
        principal: Option<Principal>,
        connection: Connection,
        out: mpsc::Sender<Arc<Vec<u8>>>,
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
        .max_idle_timeout(Some(Duration::from_secs(15).try_into().unwrap()));
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
    mut session: Session,
    options: ServerOptions,
    max_players: usize,
    admin_store: Option<AdminStore>,
    require_identity: bool,
) -> Result<ServerHandle> {
    ensure!((1..=64).contains(&max_players), "Invalid player limit");
    ensure!(
        !options.spawn_points.is_empty()
            && options.spawn_points.len() <= 256
            && !options.content_id.is_empty()
            && options.content_id.len() <= 128,
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
    session.set_ownership_scope(format!("{:x}", Sha256::digest(bytes)))?;
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
    let (out, mut output) = mpsc::channel::<Arc<Vec<u8>>>(32);
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
        while let Some(bytes) = output.recv().await {
            tokio::time::timeout(
                Duration::from_secs(10),
                codec::write_frame(&mut send, &bytes),
            )
            .await??;
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
        loop {
            let bytes = connection.read_datagram().await?;
            if bytes.len() > MAX_DATAGRAM {
                continue;
            }
            if let Ok(movement) = serde_json::from_slice::<Movement>(&bytes)
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
fn broadcast_admin_snapshots(session: &Session, peers: &BTreeMap<OwnerId, Peer>) -> Result<()> {
    for (owner, peer) in peers {
        let snapshot = session.admin_state(*owner)?;
        let bytes = Arc::new(codec::encode(&Message::AdminSnapshot(snapshot))?);
        if peer.out.try_send(bytes).is_err() {
            peer.connection
                .close(1_u32.into(), b"Reliable backlog exceeded");
        }
    }
    Ok(())
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
    let mut tickets = BTreeMap::<[u8; 32], (OwnerId, bool, Option<Principal>)>::new();
    let mut cursor = 0_u64;
    let mut names = BTreeMap::new();
    let mut avatars = BTreeMap::new();
    let mut tools = BTreeMap::new();
    let mut weapons = bri_sim::session::WeaponView::default();
    let mut palette = session.simulation().state().palette.clone();
    let mut vitals = BTreeMap::new();
    let mut minigames = Vec::new();
    let mut vehicles = Vec::new();
    let mut last_chat = 0;
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
        Some(event)=incoming.recv()=>{match event {
            Event::Join{hello,principal,connection,out,answer}=>{
                let join:Result<OwnerId>= (||{
                    ensure!(hello.version==VERSION,"Incompatible protocol version");ensure!(hello.content_id==options.content_id,"Required content does not match");
                    ensure!(peers.len()<max_players,"Server is full");
                    let supplied_host=if let Some(host)=&hello.host {ensure!(token_key(host)==host_key,"Invalid host credential");true}else{false};
                    let (owner,token)=if let Some(token)=hello.resume {
                        let (owner,ticket_host,ticket_principal)=*tickets.get(&token_key(&token)).context("Invalid resume credential")?;
                        ensure!(!peers.contains_key(&owner),"Owner is still connected");
                        ensure!(ticket_principal==principal,"Resume identity does not match authenticated ticket");
                        let administrator=ticket_host || supplied_host;
                        let mut error=None;let mut found=false;for spawn in &options.spawn_points {match session.resume_verified(owner,*spawn,administrator,principal){Ok(())=>{found=true;break},Err(e)=>error=Some(e)}}ensure!(found,"{}",error.context("No spawn points")?);
                        tickets.insert(token_key(&token),(owner,administrator,principal));
                        resumes+=1;(owner,token)
                    }else{
                        ensure!(tickets.len()<4096,"Server identity capacity reached");
                        let mut bytes=[0;32];getrandom::fill(&mut bytes).map_err(|e|anyhow::anyhow!("OS randomness failed: {e}"))?;let token=ResumeToken(bytes);
                        let administrator=supplied_host;
                        let mut owner=None;let mut error=None;for spawn in &options.spawn_points {match session.join_verified(hello.name.clone(),*spawn,administrator,principal){Ok(id)=>{owner=Some(id);break},Err(e)=>error=Some(e)}}
                        let owner=owner.ok_or_else(||error.unwrap_or_else(||anyhow::anyhow!("No spawn points")))?;
                        tickets.insert(token_key(&token),(owner,administrator,principal));joins+=1;(owner,token)
                    };
                    let checkpoint=Checkpoint::from_session(&session,cursor);
                    let encoded=match codec::encode(&Message::Welcome{owner,administrator:session.is_administrator(owner),resume:token,checkpoint}){Ok(frame)=>frame,Err(error)=>{let _=session.disconnect(owner);return Err(error)}};
                    if out.try_send(Arc::new(encoded)).is_err(){let _=session.disconnect(owner);anyhow::bail!("Join writer unavailable");}
                    peers.insert(owner,Peer{generation:connection.stable_id(),connection,out});Ok(owner)
                })();
                if join.is_err(){rejected+=1;}else{broadcast_admin_snapshots(&session,&peers)?;}let _=answer.send(join.map_err(|e|e.to_string()));
            },
            Event::Lost{owner,generation}=>{if peers.get(&owner).is_some_and(|p|p.generation==generation){peers.remove(&owner);let _=session.disconnect(owner);broadcast_admin_snapshots(&session,&peers)?;}},
            Event::Command{owner,generation,request,_body_permit}=>{
                if let Some(peer)=peers.get(&owner).filter(|p|p.generation==generation){
                    commands+=1;
                    let old_admin_revision=session.admin_revision();
                    let result=session.command_with_aim_and_admin_persistence(owner,request.sequence,request.command,request.aim,|state|match admin_store.as_mut(){Some(store)=>store.persist(state),None=>anyhow::bail!("Persistent administration storage is not configured")});
                    if result.is_err(){rejected+=1;}
                    let bytes=match codec::encode(&Message::Reply{sequence:request.sequence,result:result.map_err(|e|bri_sim::session::Rejection::from_error(&e))}) {Ok(bytes)=>bytes,Err(error)=>codec::encode(&Message::Reply{sequence:request.sequence,result:Err(bri_sim::session::Rejection::message(format!("Could not transfer reply: {error}")))})?};
                    let output=peer.out.clone();
                    let connection=peer.connection.clone();
                    if output.try_send(Arc::new(bytes)).is_err(){connection.close(1_u32.into(),b"Reliable backlog exceeded");}
                    for target in session.take_admin_disconnects(){
                        if let Some(target_peer)=peers.remove(&target){
                            target_peer.connection.close(0_u32.into(),b"Administration disconnect");
                            let _=session.disconnect(target);
                        }
                    }
                    if old_admin_revision!=session.admin_revision(){broadcast_admin_snapshots(&session,&peers)?;}
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
            let steps=clock.advance(now.duration_since(previous));previous=now;
            for _ in 0..steps {
            session.step()?;let tick=session.simulation().state().tick;
            if tick.is_multiple_of(POSE_INTERVAL) {
                for pose in poses(&session){let bytes=serde_json::to_vec(&Datagram::Pose(pose))?;for peer in peers.values(){let _=peer.connection.send_datagram(bytes.clone().into());}}
                for pose in session.vehicle_poses(){let bytes=serde_json::to_vec(&Datagram::Vehicle(pose))?;if bytes.len()<=MAX_DATAGRAM{for peer in peers.values(){let _=peer.connection.send_datagram(bytes.clone().into());}}}
            }
            if tick.is_multiple_of(6) {
                let mut bricks=BTreeMap::new();for id in session.take_dirty(){bricks.insert(id,session.simulation().state().bricks.get(&id).map(public_brick));}
                let current_avatars=session.avatars();let changed_avatars=if avatars!=current_avatars{avatars=current_avatars;Some(avatars.clone())}else{None};
                let current_tools=session.tool_inventories();let changed_tools=if tools!=current_tools{tools=current_tools;Some(tools.clone())}else{None};
                let current_weapons=session.weapon_view();let changed_weapons=if weapons!=current_weapons{weapons=current_weapons;Some(weapons.clone())}else{None};
                let current_palette=&session.simulation().state().palette;let changed_palette=if &palette!=current_palette{palette=current_palette.clone();Some(palette.clone())}else{None};
                let current_names=session.names();let changed_names=if names!=current_names{names=current_names;Some(names.clone())}else{None};
                let current_vitals=session.vitals();let changed_vitals=if vitals!=current_vitals{vitals=current_vitals;Some(vitals.clone())}else{None};
                let current_minigames=session.minigame_views();let changed_minigames=if minigames!=current_minigames{minigames=current_minigames;Some(minigames.clone())}else{None};
                let current_vehicles=session.vehicle_infos();let changed_vehicles=if vehicles!=current_vehicles{vehicles=current_vehicles;Some(vehicles.clone())}else{None};
                let chat:Vec<_>=session.chat().into_iter().filter(|c|c.id>last_chat).collect();if let Some(line)=chat.last(){last_chat=line.id;}
                let next=cursor.checked_add(1).context("Replication sequence exhausted")?;
                let cues=session.take_cues();let dropped_cues=session.dropped_cues();
                let bytes=Arc::new(codec::encode(&Message::Update(Delta{base:cursor,cursor:next,tick,bricks,names:changed_names,avatars:changed_avatars,tools:changed_tools,weapons:changed_weapons,palette:changed_palette,chat,cues,dropped_cues,vitals:changed_vitals,minigames:changed_minigames,vehicles:changed_vehicles}))?);cursor=next;
                for peer in peers.values(){if peer.out.try_send(bytes.clone()).is_err(){peer.connection.close(1_u32.into(),b"Reliable backlog exceeded");}}
                for (owner,notice) in session.take_private_notices(){if let Some(peer)=peers.get(&owner){let bytes=Arc::new(codec::encode(&Message::Notice(notice))?);if peer.out.try_send(bytes).is_err(){peer.connection.close(1_u32.into(),b"Reliable backlog exceeded");}}}
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
        final_world: Checkpoint::from_session(&session, cursor).world,
        native_world: session.simulation().state().clone(),
        notices: session.take_notices(),
    })
}
