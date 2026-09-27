//! Bounded asynchronous transport bridge. The main thread never waits on QUIC.
use anyhow::{Context, Result, ensure};
use bri_net::{
    client::{Client, ClientEvent},
    protocol::{Pose, PublicWorld},
    server::ServerHandle,
};
use bri_sim::{
    player::MoveInput,
    session::{ChatLine, Command, Reply},
};
use bri_world::OwnerId;
use std::{collections::BTreeMap, future::Future, sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot, watch};

pub struct Connected {
    pub client: Client,
    pub host: Option<ServerHandle>,
}
#[derive(Clone)]
pub struct View {
    pub weapons: bri_sim::session::WeaponView,
    pub tools: BTreeMap<OwnerId, bri_sim::session::ToolInventory>,
    pub owner: OwnerId,
    pub administrator: bool,
    pub world: Arc<PublicWorld>,
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
    pub vehicles: BTreeMap<u64, bri_sim::session::VehicleInfo>,
    pub vehicle_poses: BTreeMap<u64, bri_sim::session::VehiclePose>,
    pub rtt_ms: u32,
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
}
struct Request {
    id: u64,
    command: Command,
    aim: Option<bri_sim::session::ActionAim>,
}
pub struct Worker {
    requests: mpsc::Sender<Request>,
    movement: mpsc::Sender<(u64, Vec<MoveInput>)>,
    pub view: watch::Receiver<Option<View>>,
    pub events: mpsc::Receiver<Event>,
    stop: Option<oneshot::Sender<()>>,
}
impl Worker {
    pub fn start<F>(runtime: &tokio::runtime::Handle, connect: F) -> Self
    where
        F: Future<Output = Result<Connected>> + Send + 'static,
    {
        let (requests, rx) = mpsc::channel(64);
        let (movement, movement_rx) = mpsc::channel(32);
        let (view_tx, view) = watch::channel(None);
        let (events_tx, events) = mpsc::channel(128);
        let (stop, mut stopped) = oneshot::channel();
        runtime.spawn(async move {
            let connected=tokio::select! {
                _=&mut stopped=>return,
                result=tokio::time::timeout(Duration::from_secs(120),connect)=>result.context("Connection/content preparation timed out").and_then(|r|r),
            };
            let result=match connected {
                Ok(mut connection)=>{
                    let result=tokio::select! {
                        _=&mut stopped=>Ok(()),
                        result=run(&mut connection.client,rx,movement_rx,&view_tx,&events_tx)=>result,
                    };
                    connection.client.close();
                    if let Some(host)=connection.host.take() {
                        // Stop the host even when dispatch failed or the UI cancelled.
                        // A host persistence adapter consumes its final world later.
                        let _=host.stop().await;
                    }
                    result
                }
                Err(error)=>Err(error),
            };
            if let Err(error)=result { let _=events_tx.try_send(Event::Failed(format!("{error:#}"))); }
        });
        Self {
            requests,
            movement,
            view,
            events,
            stop: Some(stop),
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
    pub fn movement(&self, newest: u64, inputs: Vec<MoveInput>) -> Result<()> {
        for input in &inputs {
            input.validate()?;
        }
        match self.movement.try_send((newest, inputs)) {
            Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => Ok(()),
            Err(mpsc::error::TrySendError::Closed(_)) => {
                anyhow::bail!("Network worker stopped")
            }
        }
    }
    pub fn cancel(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn publish(
    client: &Client,
    world: Arc<PublicWorld>,
    checkpoint_cue_cursor: u64,
    sender: &watch::Sender<Option<View>>,
) {
    sender.send_replace(Some(View {
        weapons: client.replica.weapons.clone(),
        tools: client.replica.tools.clone(),
        owner: client.owner,
        administrator: client.administrator,
        world,
        names: client.replica.names.clone(),
        avatars: client.replica.avatars.clone(),
        poses: client.replica.poses.clone(),
        chat: client.replica.chat.iter().cloned().collect(),
        tick: client.replica.tick,
        checkpoint_cue_cursor,
        admin_snapshot: client.admin_snapshot.clone(),
        vitals: client.replica.vitals.clone(),
        minigames: client.replica.minigames.clone(),
        vehicles: client.replica.vehicles.clone(),
        vehicle_poses: client.replica.vehicle_poses.clone(),
        rtt_ms: client.rtt().as_millis().min(u128::from(u32::MAX)) as u32,
    }));
}
async fn run(
    client: &mut Client,
    mut requests: mpsc::Receiver<Request>,
    mut movement: mpsc::Receiver<(u64, Vec<MoveInput>)>,
    view: &watch::Sender<Option<View>>,
    events: &mpsc::Sender<Event>,
) -> Result<()> {
    let mut world = Arc::new(client.replica.world.clone());
    let checkpoint_cue_cursor = client.replica.cue_cursor;
    publish(client, world.clone(), checkpoint_cue_cursor, view);
    events
        .try_send(Event::Ready)
        .context("UI event queue closed")?;
    let mut pending = BTreeMap::<u64, (u64, std::time::Instant)>::new();
    let mut cue_drops = client.replica.dropped_cues;
    let mut clock = tokio::time::interval(Duration::from_millis(250));
    clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _=clock.tick()=>{
                ensure!(pending.values().all(|(_,at)|at.elapsed()<Duration::from_secs(10)),"Server request timed out");
            }
            batch=movement.recv()=>{
                let Some((newest,inputs))=batch else { return Ok(()) };
                client.movement(newest,&inputs)?;
            }
            request=requests.recv()=>{
                let Some(request)=request else { return Ok(()) };
                ensure!(pending.len()<64,"Too many pending server commands");
                let sequence=tokio::time::timeout(Duration::from_secs(10),client.request_with_aim(request.command,request.aim)).await.context("Server request write timed out")??;
                pending.insert(sequence,(request.id,std::time::Instant::now()));
            }
            incoming=client.receive()=>{
                match incoming? {
                    ClientEvent::Reply {sequence,result}=>{
                        let (request,_)=pending.remove(&sequence).context("Unsolicited server reply")?;
                        events.try_send(Event::Reply{request,result}).context("UI reply queue is full or closed")?;
                    }
                    ClientEvent::Updated {world_changed}=>{
                        let cues=client.replica.take_cues();
                        if !cues.is_empty() || client.replica.dropped_cues!=cue_drops {
                            cue_drops=client.replica.dropped_cues;
                            events.try_send(Event::Presentation{cues,dropped:client.replica.dropped_cues}).context("Client presentation queue is full or closed")?;
                        }
                        if world_changed {world=Arc::new(client.replica.world.clone());}
                        publish(client,world.clone(),checkpoint_cue_cursor,view);
                    }
                    ClientEvent::Pose(_)|ClientEvent::Vehicle(_)=>publish(client,world.clone(),checkpoint_cue_cursor,view),
                    ClientEvent::AdminSnapshot(_)=>publish(client,world.clone(),checkpoint_cue_cursor,view),
                    ClientEvent::Notice(notice)=>events.try_send(Event::Notice(notice)).context("UI notice queue is full or closed")?,
                }
            }
        }
    }
}
