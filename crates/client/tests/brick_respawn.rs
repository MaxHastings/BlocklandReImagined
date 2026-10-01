//! A rocket outside a minigame knocks free-build bricks out and they come
//! back after 30 s on every screen: the host's own and a joiner's. Each
//! screen is the app's own client pipeline (the network worker, its world
//! log, the brick query/collision mirror and the render chunks), fed over
//! real loopback QUIC. No window, GPU or OS input. See
//! docs/audits/brick-damage.md.
use anyhow::{Context, Result, bail, ensure};
use bri_client::{
    building::Building,
    network::{Connected, Event, View, Worker, WorldLog},
    world_chunks::{BrickPalette, ChunkedWorld, chunk_key},
};
use bri_net::{client::Client, protocol::PublicWorld, server};
use bri_sim::{
    definitions::Definitions,
    player::{MoveInput, PlayerState, PlayerTuning},
    presentation::CueKind,
    session::{Command, Reply, Session, ToolInventory},
    simulation::Simulation,
};
use bri_world::{BrickId, ContentRef, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

const BRICK: &str = "v20/brick/brick2x2data";
const ROCKET: &str = "v20.weapon.rocketlauncheritem";

fn content() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}
fn definitions() -> Result<Definitions> {
    Definitions::load(
        &content().join("stock-catalog-004"),
        &content().join("maps-pass-008"),
    )
}
fn ground() -> Vec<ColliderBuilder> {
    vec![ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0))]
}

/// The app's Start Game host: single player and LAN (`$Server::LAN`), and
/// a spawn loadout that carries the rocket launcher.
fn host() -> Result<server::ServerHandle> {
    let mut session = Session::new(Simulation::new(
        World::new("Free build".into(), "fixture".into(), vec![[1.0; 4]; 2]),
        definitions()?,
        ground(),
    )?);
    session.set_lan_host(true);
    let pack = std::fs::read(content().join("weapons-pack-009/weapons.json"))?;
    session.set_weapon_pack(bri_weapons::Pack::from_json(&pack)?)?;
    let mut loadout = ToolInventory::default();
    loadout.slots[3] = Some(ROCKET.into());
    session.set_spawn_loadout(loadout)?;
    server::start(
        session,
        server::ServerOptions {
            bind: "127.0.0.1:0".parse()?,
            environment: bri_package::environment::Environment::empty(),
            spawn_points: vec![Vec3::new(-48.0, 0.05, 0.0), Vec3::new(-24.0, 0.05, 0.0)],
            certificate: None,
            map_loader: None,
            packages: None,
        },
    )
}

/// One player's client: what the app keeps for drawing and colliding.
struct Screen {
    name: &'static str,
    worker: Worker,
    building: Building,
    chunks: ChunkedWorld,
    meshes: BTreeMap<String, bri_content::brick::Brick>,
    palette: BrickPalette,
    applied: Option<(Arc<PublicWorld>, Arc<WorldLog>, u64)>,
    replies: BTreeMap<u64, std::result::Result<Reply, String>>,
    debris: Vec<BrickId>,
    next_request: u64,
}

impl Screen {
    async fn open(name: &'static str, connected: Connected) -> Result<Self> {
        let mut worker = Worker::start(
            &tokio::runtime::Handle::current(),
            async move { Ok(connected) },
        );
        match tokio::time::timeout(Duration::from_secs(10), worker.events.recv()).await? {
            Some(Event::Ready) => {}
            Some(Event::Failed(e)) => bail!("{name}: {e}"),
            _ => bail!("{name}: no ready event"),
        }
        let meshes = definitions()?
            .entries
            .into_iter()
            .map(|(id, d)| (id, d.mesh))
            .collect();
        Ok(Self {
            name,
            worker,
            building: Building::new(definitions()?, ground())?,
            chunks: ChunkedWorld::default(),
            meshes,
            palette: BrickPalette::development(),
            applied: None,
            replies: BTreeMap::new(),
            debris: Vec::new(),
            next_request: 1,
        })
    }
    fn view(&self) -> View {
        self.worker
            .view
            .borrow()
            .clone()
            .expect("a view after Ready")
    }
    fn owner(&self) -> u64 {
        self.view().owner
    }
    /// Drain events and bring the query mirror and chunks up to the latest
    /// replica, the way the app does each frame.
    fn pump(&mut self) -> Result<()> {
        while let Ok(event) = self.worker.events.try_recv() {
            match event {
                Event::Failed(e) => bail!("{}: {e}", self.name),
                Event::Reply { request, result } => {
                    self.replies
                        .insert(request, result.map_err(|r| format!("{r:?}")));
                }
                Event::Presentation { cues, .. } => {
                    self.debris.extend(cues.iter().filter_map(|c| match c.kind {
                        CueKind::BrickKill { brick, .. } => Some(brick),
                        _ => None,
                    }));
                }
                _ => {}
            }
        }
        let view = self.view();
        if self
            .applied
            .as_ref()
            .is_some_and(|(world, _, _)| Arc::ptr_eq(world, &view.world))
        {
            return Ok(());
        }
        let known = self
            .applied
            .as_ref()
            .filter(|(_, log, _)| Arc::ptr_eq(log, &view.world_log))
            .and_then(|(_, log, revision)| log.between(*revision, view.world_revision));
        self.building
            .sync_world_changes(&view.world, known.as_ref())?;
        self.chunks.update(
            view.world.clone(),
            known.as_ref(),
            &self.meshes,
            &self.palette,
            None,
            4_000_000,
        )?;
        self.applied = Some((view.world, view.world_log, view.world_revision));
        Ok(())
    }
    fn request(&mut self, command: Command) -> Result<u64> {
        let id = self.next_request;
        self.next_request += 1;
        self.worker.request(id, command)?;
        Ok(id)
    }
    /// Is the brick drawn, solid to walk into, and in the replica at all?
    fn sees(&self, id: BrickId, at: Vec3) -> (bool, bool) {
        let drawn = self.chunks.chunk_bricks(chunk_key(at.to_array())) > 0;
        let solid = self
            .building
            .colliding_bricks(at - Vec3::splat(0.05), at + Vec3::splat(0.05))
            .is_ok_and(|hits| hits.iter().any(|(hit, _, _)| *hit == id));
        (drawn, solid)
    }
}

/// Pump both screens until `ready` holds for both, or `limit` passes.
async fn until(
    screens: &mut [&mut Screen; 2],
    what: &str,
    limit: Duration,
    ready: impl Fn(&Screen) -> bool,
) -> Result<()> {
    let start = Instant::now();
    loop {
        for screen in screens.iter_mut() {
            screen.pump()?;
        }
        if screens.iter().all(|s| ready(s)) {
            return Ok(());
        }
        ensure!(start.elapsed() < limit, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn reply(screens: &mut [&mut Screen; 2], who: usize, request: u64) -> Result<Reply> {
    until(screens, "a reply", Duration::from_secs(5), |_| true).await?;
    let start = Instant::now();
    loop {
        screens[who].pump()?;
        if let Some(result) = screens[who].replies.remove(&request) {
            return result.map_err(anyhow::Error::msg);
        }
        ensure!(start.elapsed() < Duration::from_secs(5), "no reply");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn plant(screens: &mut [&mut Screen; 2], who: usize, at: Vec3) -> Result<BrickId> {
    let request = screens[who].request(Command::Plant {
        definition: BRICK.into(),
        position: at.to_array(),
        quarter_turns: 0,
        color: 1,
    })?;
    match reply(screens, who, request).await? {
        Reply::Planted(id) => Ok(id),
        other => bail!("expected a plant, got {other:?}"),
    }
}

/// Equip the rocket launcher, face `target` and fire once.
async fn fire(screens: &mut [&mut Screen; 2], who: usize, target: Vec3) -> Result<()> {
    let owner = screens[who].owner();
    let request = screens[who].request(Command::EquipTool { slot: Some(3) })?;
    reply(screens, who, request).await?;
    until(screens, "the launcher ready", Duration::from_secs(5), |s| {
        let view = s.view();
        view.poses.get(&owner).is_some_and(|p| p.player.grounded)
            && view
                .weapons
                .images
                .get(&owner)
                .is_some_and(|images| images.iter().any(|i| i.state == "Ready"))
    })
    .await?;
    let player = screens[who].view().poses[&owner].player.clone();
    let flat = target - Vec3::from(player.feet);
    let yaw = flat.x.atan2(-flat.z);
    let d = target
        - PlayerState {
            yaw,
            ..player.clone()
        }
        .eye(&PlayerTuning::default());
    let look = MoveInput {
        yaw,
        pitch: d.y.atan2(Vec3::new(d.x, 0.0, d.z).length()),
        ..Default::default()
    };
    let sequence = screens[who].view().poses[&owner].acknowledged_input + 1;
    screens[who].worker.movement(sequence, vec![look], None, None)?;
    until(screens, "the aim", Duration::from_secs(5), |s| {
        s.view().owner != owner || s.view().poses[&owner].acknowledged_input >= sequence
    })
    .await?;
    for down in [true, false] {
        let request = screens[who].request(Command::WeaponTrigger { down })?;
        reply(screens, who, request).await?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the converted stock catalog and native weapons pack; ~35 s of real time"]
async fn rocketed_free_build_bricks_vanish_and_come_back_on_host_and_joiner_screens() -> Result<()>
{
    let server = host()?;
    let (address, certificate) = (server.address, server.certificate.clone());
    // The host plays through its own loopback client, as Start Game does.
    let client = Client::connect(address, &certificate, "Host".into(), Vec::new(), None).await?;
    let mut host = Screen::open(
        "host",
        Connected {
            client,
            host: Some(server),
            mods: Default::default(),
            package_save: None,
        },
    )
    .await?;
    let client = Client::connect(address, &certificate, "Joiner".into(), Vec::new(), None).await?;
    let mut joiner = Screen::open(
        "joiner",
        Connected {
            client,
            host: None,
            mods: Default::default(),
            package_save: None,
        },
    )
    .await?;
    let screens = &mut [&mut host, &mut joiner];

    // Two free-build bricks in separate render chunks, far enough apart
    // that one rocket reaches only its own. No minigame exists.
    let spots = [Vec3::new(-40.0, 0.3, -10.0), Vec3::new(-30.0, 0.3, -10.0)];
    let bricks = [
        plant(screens, 0, spots[0]).await?,
        plant(screens, 1, spots[1]).await?,
    ];
    let everywhere = |s: &Screen, drawn: bool, solid: bool| {
        bricks
            .iter()
            .zip(spots)
            .all(|(id, at)| s.sees(*id, at) == (drawn, solid))
    };
    until(
        screens,
        "both bricks standing",
        Duration::from_secs(5),
        |s| everywhere(s, true, true),
    )
    .await?;
    for screen in screens.iter_mut() {
        screen.debris.clear();
    }

    // The host rockets its brick, then the joiner rockets theirs.
    fire(screens, 0, spots[0]).await?;
    fire(screens, 1, spots[1]).await?;
    until(
        screens,
        "both bricks knocked out",
        Duration::from_secs(10),
        |s| everywhere(s, false, false),
    )
    .await?;
    let gone = Instant::now();
    for screen in screens.iter() {
        let view = screen.view();
        for id in bricks {
            ensure!(
                screen.debris.contains(&id),
                "{} threw no debris for brick {id}",
                screen.name
            );
            // Knocked out, not deleted: the replica still holds it.
            let b = view.world.bricks.get(&id).context("brick deleted")?;
            ensure!(!b.visible && !b.colliding && !b.raycast);
            ensure!(b.definition == ContentRef::Resolved(BRICK.into()));
        }
    }

    // Still out well into the 30 s on both screens...
    let early = until(screens, "an early return", Duration::from_secs(20), |s| {
        !everywhere(s, false, false)
    })
    .await;
    ensure!(
        early.is_err(),
        "a brick came back after only {:?}",
        gone.elapsed()
    );
    // ...then drawn and solid again on both.
    until(screens, "both bricks back", Duration::from_secs(20), |s| {
        everywhere(s, true, true)
    })
    .await?;
    let back = gone.elapsed();
    ensure!(back >= Duration::from_secs(25), "back after only {back:?}");
    for screen in screens.iter() {
        let view = screen.view();
        for id in bricks {
            ensure!(view.world.bricks[&id].raycast, "brick {id} not clickable");
        }
    }
    for screen in screens.iter_mut() {
        if let Some(task) = screen.worker.finish() {
            task.await?;
        }
    }
    Ok(())
}
