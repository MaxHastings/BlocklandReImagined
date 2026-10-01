//! Bounded asynchronous transport bridge. The main thread never waits on QUIC.
use anyhow::{Context, Result};
use bri_net::{
    client::{Client, ClientEvent},
    protocol::{Pose, PublicWorld},
    server::ServerHandle,
};
use bri_sim::{
    player::MoveInput,
    session::{CameraView, ChatLine, Command, Reply},
};
use bri_world::OwnerId;
use std::{collections::BTreeMap, future::Future, sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot, watch};

pub struct Connected {
    pub client: Client,
    pub host: Option<ServerHandle>,
    /// Add-On packages downloaded from this server and loaded for it.
    pub mods: Arc<bri_package_runtime::Catalog>,
}
#[derive(Clone)]
pub struct View {
    /// The host's listing from the handshake: the server's name and size,
    /// as the join list shows them.
    pub listing: bri_net::protocol::Listing,
    /// The host's identity for per-server trust (`addon-trust.json`):
    /// `host-key:` and the hex of its certificate's key. Not the address,
    /// which another host can take over.
    pub host_key: String,
    pub weapons: bri_sim::session::WeaponView,
    pub tools: BTreeMap<OwnerId, bri_sim::session::ToolInventory>,
    pub owner: OwnerId,
    pub administrator: bool,
    pub world: Arc<PublicWorld>,
    /// Increments with every replica world change; `world_log` says what changed.
    pub world_revision: u64,
    pub world_log: Arc<WorldLog>,
    pub names: BTreeMap<OwnerId, String>,
    pub avatars: BTreeMap<OwnerId, bri_content::avatar::Appearance>,
    pub poses: BTreeMap<OwnerId, Pose>,
    pub chat: Vec<ChatLine>,
    pub tick: u64,
    /// Immutable cue high-water mark from the join checkpoint. The live replica
    /// cursor advances as deltas arrive and must not be used to reset consumers.
    pub checkpoint_cue_cursor: u64,
    pub admin_snapshot: Option<bri_sim::session::AdminSnapshot>,
    pub vitals: BTreeMap<OwnerId, bri_sim::session::Vitals>,
    pub minigames: Vec<bri_sim::session::MiniGameView>,
    /// Admin `/timeScale`: game time per real second.
    pub time_scale: f32,
    /// Scene nodes of map shapes players have smashed.
    pub broken_shapes: std::collections::BTreeSet<u32>,
    /// The Tutorial's targets on the range.
    pub targets: Vec<bri_sim::tutorial::TargetView>,
    /// Add-On map light rules, oldest first.
    pub map_lights: Vec<bri_sim::session::MapLightRule>,
    pub vehicles: BTreeMap<u64, bri_sim::session::VehicleInfo>,
    pub vehicle_poses: BTreeMap<u64, bri_sim::session::VehiclePose>,
    /// The host's player archetypes; poses name them by index.
    pub archetypes: Arc<bri_sim::archetype::Archetypes>,
    /// Add-On packages downloaded from this server (models, HUD panels).
    pub mods: Arc<bri_package_runtime::Catalog>,
    /// Admin free-camera orbs by owner (`cameraImage`).
    pub orbs: BTreeMap<OwnerId, bri_net::protocol::Orb>,
    pub rtt_ms: u32,
    /// Entities of the server's packages.
    pub entities: Arc<BTreeMap<u64, bri_sim::session::EntityInfo>>,
    /// Public state of the server's packages.
    pub package_state: Arc<bri_sim::session::PackageStateView>,
}
/// Brick ids each replica world revision changed, so consumers can update in
/// proportion to a change instead of comparing every brick. Bounded: a
/// consumer that falls further behind compares whole worlds instead.
#[derive(Default)]
pub struct WorldLog {
    edits: std::sync::Mutex<std::collections::VecDeque<WorldEdit>>,
}
struct WorldEdit {
    revision: u64,
    bricks: Vec<u64>,
    palette: bool,
}
/// Everything that may differ between two revisions of one replica.
#[derive(Debug, Default, PartialEq)]
pub struct WorldChanges {
    pub bricks: std::collections::BTreeSet<u64>,
    pub palette: bool,
}
impl WorldLog {
    const EDITS: usize = 1024;
    fn push(&self, revision: u64, bricks: Vec<u64>, palette: bool) {
        let mut edits = self.edits.lock().unwrap_or_else(|e| e.into_inner());
        if edits.len() == Self::EDITS {
            edits.pop_front();
        }
        edits.push_back(WorldEdit {
            revision,
            bricks,
            palette,
        });
    }
    /// Changes after revision `from` through `to`, or None when that history
    /// was trimmed (or the revisions are not from this log).
    pub fn between(&self, from: u64, to: u64) -> Option<WorldChanges> {
        let edits = self.edits.lock().unwrap_or_else(|e| e.into_inner());
        let mut changes = WorldChanges::default();
        let mut expected = from + 1;
        for edit in edits
            .iter()
            .filter(|e| e.revision > from && e.revision <= to)
        {
            if edit.revision != expected {
                return None;
            }
            expected += 1;
            changes.bricks.extend(&edit.bricks);
            changes.palette |= edit.palette;
        }
        (expected == to + 1).then_some(changes)
    }
}
pub enum Event {
    Presentation {
        cues: Vec<bri_sim::presentation::Cue>,
        dropped: u64,
    },
    Ready,
    Notice(bri_sim::session::Notice),
    Reply {
        request: u64,
        result: std::result::Result<Reply, bri_sim::session::Rejection>,
    },
    Failed(String),
    /// The host changed to this map.
    MapChanged(String),
}
/// Room in the UI event queue. Replies (at most `MAX_PENDING` in flight) and
/// a failure always fit: presentation cues and notices use only what is left.
const EVENT_QUEUE: usize = 256;
const MAX_PENDING: usize = 64;
/// A request the host has not answered in this long is given up: past the
/// screens' own deadlines (`bri_ui::ui::Pending::timeout_ms`, 3 min at most),
/// so the game has already told the player. Slow answers (a big Save Bricks
/// on a busy host) are not a lost connection; QUIC's keep-alive and idle
/// timeout decide that.
const REQUEST_EXPIRY: Duration = Duration::from_secs(200);
const REPLY_ROOM: usize = MAX_PENDING + 2;
const RESERVED_EVENTS: usize = REPLY_ROOM + 32;
struct Request {
    id: u64,
    command: Command,
    aim: Option<bri_sim::session::ActionAim>,
}
/// What the net graph and performance overlay sample, once connected.
#[derive(Clone)]
pub struct Probes {
    pub link: bri_net::client::LinkProbe,
    /// The in-process server's performance when this game hosts.
    pub host: Option<Arc<std::sync::Mutex<bri_net::server::ServerPerf>>>,
}
pub struct Worker {
    pub probes: Arc<std::sync::OnceLock<Probes>>,
    requests: mpsc::Sender<Request>,
    movement: mpsc::Sender<(u64, Vec<MoveInput>, Option<CameraView>)>,
    pub view: watch::Receiver<Option<View>>,
    pub events: mpsc::Receiver<Event>,
    stop: Option<oneshot::Sender<()>>,
    /// The transport task; it ends after the host (if any) stopped and its
    /// final world was kept.
    task: Option<tokio::task::JoinHandle<()>>,
}
impl Worker {
    pub fn start<F>(runtime: &tokio::runtime::Handle, connect: F) -> Self
    where
        F: Future<Output = Result<Connected>> + Send + 'static,
    {
        let (requests, rx) = mpsc::channel(64);
        let (movement, movement_rx) = mpsc::channel(32);
        let (view_tx, view) = watch::channel(None);
        let (events_tx, events) = mpsc::channel(EVENT_QUEUE);
        let (stop, mut stopped) = oneshot::channel();
        let probes = Arc::new(std::sync::OnceLock::new());
        let probes_tx = probes.clone();
        let task = runtime.spawn(async move {
            let connected=tokio::select! {
                _=&mut stopped=>return,
                result=tokio::time::timeout(Duration::from_secs(120),connect)=>result.context("Connection/content preparation timed out").and_then(|r|r),
            };
            let result=match connected {
                Ok(mut connection)=>{
                    let _=probes_tx.set(Probes{link:connection.client.link_probe(),host:connection.host.as_ref().map(|h|h.perf.clone())});
                    let result=tokio::select! {
                        _=&mut stopped=>Ok(()),
                        result=run(&mut connection.client,connection.mods.clone(),rx,movement_rx,&view_tx,&events_tx)=>result,
                    };
                    connection.client.close();
                    if let Some(host)=connection.host.take() {
                        // Stop the host even when dispatch failed or the UI cancelled;
                        // the host keeps the package state it ends with.
                        if let Err(error)=host.stop().await {
                            bri_console::warn(format!("Host stopped with an error: {error:#}"));
                        }
                    }
                    result
                }
                Err(error)=>Err(error),
            };
            // Wait for room: a full queue must not swallow why the game ended.
            if let Err(error)=result {
                let failed=events_tx.send(Event::Failed(format!("{error:#}")));
                let _=tokio::time::timeout(Duration::from_secs(5),failed).await;
            }
        });
        Self {
            probes,
            requests,
            movement,
            view,
            events,
            stop: Some(stop),
            task: Some(task),
        }
    }
    pub fn request(&self, id: u64, command: Command) -> Result<()> {
        self.request_with_aim(id, command, None)
    }
    pub fn request_with_aim(
        &self,
        id: u64,
        command: Command,
        aim: Option<bri_sim::session::ActionAim>,
    ) -> Result<()> {
        if let Some(aim) = aim {
            aim.validate()?;
        }
        self.requests
            .try_send(Request { id, command, aim })
            .context("Network request queue is busy or disconnected")
    }
    /// Queue a redundant input datagram (newest sequence, recent inputs).
    /// A full queue drops it; the next datagram repeats these inputs anyway.
    /// A stopped worker drops it too: its `Event::Failed` (or the closed
    /// event queue) tells the game why, where the player can read it.
    pub fn movement(
        &self,
        newest: u64,
        inputs: Vec<MoveInput>,
        camera: Option<CameraView>,
    ) -> Result<()> {
        for input in &inputs {
            input.validate()?;
        }
        let _ = self.movement.try_send((newest, inputs, camera));
        Ok(())
    }
    pub fn cancel(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
    /// Cancel, and hand back the task so a quitting game can wait for the
    /// host to stop and keep its world.
    pub fn finish(&mut self) -> Option<tokio::task::JoinHandle<()>> {
        self.cancel();
        self.task.take()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Requests sent to the host and not answered yet, by wire sequence.
#[derive(Default)]
struct InFlight(BTreeMap<u64, (u64, std::time::Instant)>);
impl InFlight {
    fn full(&self) -> bool {
        self.0.len() >= MAX_PENDING
    }
    fn sent(&mut self, sequence: u64, request: u64, at: std::time::Instant) {
        self.0.insert(sequence, (request, at));
    }
    /// The UI request a reply answers, or None for one given up on.
    fn answered(&mut self, sequence: u64) -> Option<u64> {
        self.0.remove(&sequence).map(|(request, _)| request)
    }
    /// Give up on requests older than `REQUEST_EXPIRY`: their UI requests.
    fn expire(&mut self, now: std::time::Instant) -> Vec<u64> {
        let mut expired = Vec::new();
        self.0.retain(|_, (request, at)| {
            let keep = now.saturating_duration_since(*at) < REQUEST_EXPIRY;
            if !keep {
                expired.push(*request);
            }
            keep
        });
        expired
    }
}
struct WorldState {
    world: Arc<PublicWorld>,
    revision: u64,
    log: Arc<WorldLog>,
    mods: Arc<bri_package_runtime::Catalog>,
}
/// How per-server trust names the host that presented `certificate`.
pub fn host_trust_key(certificate: &[u8]) -> String {
    format!(
        "host-key:{}",
        bri_net::discovery::hex(&bri_net::invite::host_key(certificate))
    )
}
fn publish(
    client: &Client,
    host_key: &str,
    world: &WorldState,
    checkpoint_cue_cursor: u64,
    sender: &watch::Sender<Option<View>>,
) {
    sender.send_replace(Some(View {
        listing: client.listing.clone(),
        host_key: host_key.to_owned(),
        weapons: client.replica.weapons.clone(),
        tools: client.replica.tools.clone(),
        owner: client.owner,
        administrator: client.administrator,
        world: world.world.clone(),
        world_revision: world.revision,
        world_log: world.log.clone(),
        names: client.replica.names.clone(),
        avatars: client.replica.avatars.clone(),
        poses: client.replica.poses.clone(),
        chat: client.replica.chat.iter().cloned().collect(),
        tick: client.replica.tick,
        checkpoint_cue_cursor,
        admin_snapshot: client.admin_snapshot.clone(),
        vitals: client.replica.vitals.clone(),
        minigames: client.replica.minigames.clone(),
        time_scale: client.replica.time_scale,
        broken_shapes: client.replica.broken_shapes.clone(),
        targets: client.replica.targets.clone(),
        map_lights: client.replica.map_lights.clone(),
        vehicles: client.replica.vehicles.clone(),
        vehicle_poses: client.replica.vehicle_poses.clone(),
        archetypes: client.replica.archetypes.clone(),
        mods: world.mods.clone(),
        orbs: client.replica.orbs.clone(),
        rtt_ms: client.rtt().as_millis().min(u128::from(u32::MAX)) as u32,
        entities: Arc::new(client.replica.entities.clone()),
        package_state: Arc::new(client.replica.package_state.clone()),
    }));
}
async fn run(
    client: &mut Client,
    mods: Arc<bri_package_runtime::Catalog>,
    mut requests: mpsc::Receiver<Request>,
    mut movement: mpsc::Receiver<(u64, Vec<MoveInput>, Option<CameraView>)>,
    view: &watch::Sender<Option<View>>,
    events: &mpsc::Sender<Event>,
) -> Result<()> {
    let mut world = WorldState {
        world: Arc::new(client.replica.world.clone()),
        revision: 0,
        log: Arc::default(),
        mods,
    };
    let checkpoint_cue_cursor = client.replica.cue_cursor;
    let host_key = host_trust_key(&client.certificate);
    publish(client, &host_key, &world, checkpoint_cue_cursor, view);
    // The view is a full copy of the replica, and one datagram carries
    // many poses. A change marks the view stale; it is published once the
    // messages that were already queued behind it are handled, and before
    // anything else reaches the UI, so the UI never sees an older view than
    // the events it gets.
    let mut stale = false;
    let mut behind = 0_usize;
    events
        .try_send(Event::Ready)
        .context("UI event queue closed")?;
    let mut pending = InFlight::default();
    let mut cue_drops = client.replica.dropped_cues;
    // Cues dropped here because the UI fell behind.
    let mut local_drops = 0_u64;
    let mut clock = tokio::time::interval(Duration::from_millis(250));
    clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Inputs held until MOVEMENT_GAP has passed since the last datagram.
    type Batch = (u64, Vec<MoveInput>);
    let mut held: Option<(Batch, Option<CameraView>)> = None;
    let mut last_movement = tokio::time::Instant::now();
    let mut sent_newest = 0_u64;
    // Everything not sent yet, plus the usual redundancy: a held frame costs
    // no extra datagram.
    let mut send = |client: &mut Client, (newest, mut inputs): (u64, Vec<MoveInput>), camera| {
        let unsent = newest.saturating_sub(sent_newest) as usize;
        let excess = inputs
            .len()
            .saturating_sub(unsent.max(bri_net::protocol::MOVEMENT_REDUNDANCY));
        inputs.drain(..excess);
        sent_newest = sent_newest.max(newest);
        client.movement(newest, &inputs, camera)
    };
    loop {
        let release = last_movement + bri_net::protocol::MOVEMENT_GAP;
        tokio::select! {
            _=clock.tick()=>{
                for request in pending.expire(std::time::Instant::now()) {
                    bri_console::warn(format!("The server never answered request {request}; giving up on it"));
                    let result=Err(bri_sim::session::Rejection{plant:None,message:"The server did not answer in time.".into()});
                    events.try_send(Event::Reply{request,result}).context("UI reply queue is full or closed")?;
                }
            }
            batch=movement.recv()=>{
                let Some((newest,inputs,camera))=batch else { return Ok(()) };
                let batch=match held.take() {
                    Some((older,_))=>bri_net::protocol::merge_movement(older,(newest,inputs)),
                    None=>(newest,inputs),
                };
                if tokio::time::Instant::now()>=release {
                    send(client,batch,camera)?;
                    last_movement=tokio::time::Instant::now();
                } else {
                    held=Some((batch,camera));
                }
            }
            _=tokio::time::sleep_until(release),if held.is_some()=>{
                if let Some((batch,camera))=held.take() {
                    send(client,batch,camera)?;
                    last_movement=tokio::time::Instant::now();
                }
            }
            request=requests.recv()=>{
                let Some(request)=request else { return Ok(()) };
                if pending.full() {
                    // Refuse this one; the connection and the answers on
                    // their way are fine.
                    let result=Err(bri_sim::session::Rejection{plant:None,message:"Too many requests are waiting on the server; try again in a moment.".into()});
                    events.try_send(Event::Reply{request:request.id,result}).context("UI reply queue is full or closed")?;
                    continue;
                }
                let sequence=tokio::time::timeout(Duration::from_secs(10),client.request_with_aim(request.command,request.aim)).await.context("Server request write timed out")??;
                pending.sent(sequence,request.id,std::time::Instant::now());
            }
            incoming=client.receive()=>{
                let starts_batch=behind==0;
                match incoming? {
                    ClientEvent::Reply {sequence,result}=>{
                        // An answer to a request given up on above is dropped.
                        let Some(request)=pending.answered(sequence) else {
                            bri_console::warn(format!("Dropped a late server answer ({sequence})"));
                            continue;
                        };
                        if std::mem::take(&mut stale) { publish(client,&host_key,&world,checkpoint_cue_cursor,view); }
                        events.try_send(Event::Reply{request,result}).context("UI reply queue is full or closed")?;
                    }
                    ClientEvent::Updated {world_changed,changed_bricks,palette_changed}=>{
                        let cues=client.replica.take_cues();
                        let dropped=client.replica.dropped_cues.saturating_add(local_drops);
                        if !cues.is_empty() || dropped!=cue_drops {
                            // Effects the UI has no room for are dropped and
                            // counted, as the host drops its own excess; they
                            // never take the room replies need.
                            if events.capacity()>RESERVED_EVENTS {
                                cue_drops=dropped;
                                if std::mem::take(&mut stale) { publish(client,&host_key,&world,checkpoint_cue_cursor,view); }
                                events.try_send(Event::Presentation{cues,dropped}).context("Client presentation queue is full or closed")?;
                            } else {
                                local_drops=local_drops.saturating_add(cues.len() as u64);
                            }
                        }
                        if world_changed {
                            world.revision+=1;
                            world.log.push(world.revision,changed_bricks,palette_changed);
                            world.world=Arc::new(client.replica.world.clone());
                        }
                        stale=true;
                    }
                    ClientEvent::Pose(_)|ClientEvent::Vehicle(_)|ClientEvent::Orb(_)|ClientEvent::AdminSnapshot(_)=>stale=true,
                    // The loading screen comes up from bri-progress; the replica swaps on MapChanged.
                    ClientEvent::MapChanging{..}=>{}
                    ClientEvent::MapChanged=>{
                        // No log entry: consumers compare the whole new world.
                        world.revision+=1;
                        world.world=Arc::new(client.replica.world.clone());
                        stale=false;
                        publish(client,&host_key,&world,checkpoint_cue_cursor,view);
                        events.try_send(Event::MapChanged(client.replica.world.map_id.clone())).context("UI event queue is full or closed")?;
                    }
                    // A flood of notices ("Too many events at once!") drops
                    // the excess rather than the connection.
                    ClientEvent::Notice(notice)=>if events.capacity()>REPLY_ROOM {
                        if std::mem::take(&mut stale) { publish(client,&host_key,&world,checkpoint_cue_cursor,view); }
                        events.try_send(Event::Notice(notice)).context("UI notice queue is full or closed")?
                    },
                }
                behind=if starts_batch { client.queued() } else { behind-1 };
                if behind==0 && std::mem::take(&mut stale) {
                    publish(client,&host_key,&world,checkpoint_cue_cursor,view);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_slow_or_lost_answer_costs_its_request_not_the_connection() {
        let start = std::time::Instant::now();
        let mut in_flight = InFlight::default();
        for n in 0..MAX_PENDING as u64 {
            in_flight.sent(n + 1, 100 + n, start);
        }
        // Full: the next request is refused on its own.
        assert!(in_flight.full());
        // A slow answer well past the old 10 s limit still arrives.
        assert!(in_flight.expire(start + Duration::from_secs(60)).is_empty());
        assert_eq!(in_flight.answered(1), Some(100));
        assert!(!in_flight.full());
        // Past the screens' deadlines the rest are given up on, once.
        let expired = in_flight.expire(start + REQUEST_EXPIRY);
        assert_eq!(expired.len(), MAX_PENDING - 1);
        assert!(in_flight.expire(start + REQUEST_EXPIRY * 2).is_empty());
        // Their late answers are dropped, not treated as a broken host.
        assert_eq!(in_flight.answered(2), None);
    }
    #[test]
    fn world_log_reports_contiguous_changes_or_nothing() {
        let log = WorldLog::default();
        log.push(1, vec![5, 6], false);
        log.push(2, vec![6, 7], true);
        assert_eq!(
            log.between(0, 2),
            Some(WorldChanges {
                bricks: [5, 6, 7].into(),
                palette: true
            })
        );
        assert_eq!(log.between(2, 2), Some(WorldChanges::default()));
        assert_eq!(log.between(1, 3), None, "future revision");
        for revision in 3..=WorldLog::EDITS as u64 + 2 {
            log.push(revision, vec![revision], false);
        }
        assert_eq!(log.between(0, 5), None, "trimmed history");
        let recent = log.between(10, 12).unwrap();
        assert_eq!(recent.bricks, [11, 12].into());
    }
}
