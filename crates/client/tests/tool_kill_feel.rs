//! How long a destroyed brick stays in sight and in the way, by cause, on
//! every screen: the host's own (single player and LAN host share this
//! path) and a joiner's over real loopback QUIC. The host hammers a brick,
//! the joiner Destructo-Wands one, and the host rockets one inside a Brick
//! Damage minigame. Each screen turns the kills it receives into the app's
//! own `BrickDebris` against its own brick mirror and steps it at 60 fps.
//!
//! v20 (`blocklandv20.exe`, see docs/audits/brick-damage.md): `killBrick`
//! (hammer, wands) never collides: the brick hops, spins and falls through
//! the world, fading after 0.5 s. Brick explosions (`transmitBrickExplosion`)
//! are physics bodies that tumble against the world.
use anyhow::{Context, Result, bail, ensure};
use bri_client::{
    brick_debris::BrickDebris,
    building::Building,
    network::{Connected, Event, View, Worker, WorldLog},
};
use bri_net::{client::Client, protocol::PublicWorld, server};
use bri_sim::{
    definitions::Definitions,
    player::{MoveInput, PlayerState, PlayerTuning},
    presentation::{BrickDeath, Cue, CueKind},
    session::{Command, MiniGameRequest, Reply, Session, ToolInventory},
    simulation::Simulation,
};
use bri_world::{BrickId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

const BRICK: &str = "v20/brick/brick2x2data";
const HAMMER: &str = "v20.weapon.hammeritem";
const ROCKET: &str = "v20.weapon.rocketlauncheritem";
/// Frame time the debris is stepped at.
const FRAME: f32 = 1.0 / 60.0;
/// A piece fainter than this is gone to the eye.
const SEEN: f32 = 0.05;
/// A visible piece this close to where its brick stood is in the way of
/// seeing (and rebuilding) what was destroyed.
const IN_THE_WAY: f32 = 2.0;

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

/// The app's Start Game host (single player and LAN): hammer and rocket
/// in the spawn loadout; the Destructo Wand is `/wand`.
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
    loadout.slots[0] = Some(HAMMER.into());
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

struct Screen {
    name: &'static str,
    worker: Worker,
    building: Building,
    applied: Option<(Arc<PublicWorld>, Arc<WorldLog>, u64)>,
    replies: std::collections::BTreeMap<u64, std::result::Result<Reply, String>>,
    kills: Vec<Cue>,
    next_request: u64,
}

impl Screen {
    async fn open(name: &'static str, connected: Connected) -> Result<Self> {
        let mut worker = Worker::start(
            &tokio::runtime::Handle::current(),
            Default::default(),
            async move { Ok(connected) },
        );
        match tokio::time::timeout(Duration::from_secs(10), worker.events.recv()).await? {
            Some(Event::Ready) => {}
            Some(Event::Failed(e)) => bail!("{name}: {e}"),
            _ => bail!("{name}: no ready event"),
        }
        Ok(Self {
            name,
            worker,
            building: Building::new(definitions()?, ground())?,
            applied: None,
            replies: Default::default(),
            kills: Vec::new(),
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
    fn pump(&mut self) -> Result<()> {
        while let Ok(event) = self.worker.events.try_recv() {
            match event {
                Event::Failed(e) => bail!("{}: {e}", self.name),
                Event::Reply { request, result } => {
                    self.replies
                        .insert(request, result.map_err(|r| format!("{r:?}")));
                }
                Event::Presentation { cues, .. } => self.kills.extend(
                    cues.iter()
                        .filter(|c| matches!(c.kind, CueKind::BrickKill { .. }))
                        .cloned(),
                ),
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
        self.applied = Some((view.world, view.world_log, view.world_revision));
        Ok(())
    }
    fn request(&mut self, command: Command) -> Result<u64> {
        let id = self.next_request;
        self.next_request += 1;
        self.worker.request(id, command)?;
        Ok(id)
    }
    fn feet(&self) -> Vec3 {
        let view = self.view();
        Vec3::from(view.poses[&view.owner].player.feet)
    }
    fn kill(&self, brick: BrickId) -> Option<&Cue> {
        self.kills
            .iter()
            .find(|c| matches!(c.kind, CueKind::BrickKill { brick: b, .. } if b == brick))
    }
}

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
    let start = Instant::now();
    loop {
        for screen in screens.iter_mut() {
            screen.pump()?;
        }
        if let Some(result) = screens[who].replies.remove(&request) {
            return result.map_err(anyhow::Error::msg);
        }
        ensure!(start.elapsed() < Duration::from_secs(5), "no reply");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn command(screens: &mut [&mut Screen; 2], who: usize, command: Command) -> Result<Reply> {
    let request = screens[who].request(command)?;
    reply(screens, who, request).await
}

async fn plant(screens: &mut [&mut Screen; 2], who: usize, at: Vec3) -> Result<BrickId> {
    match command(
        screens,
        who,
        Command::Plant {
            definition: BRICK.into(),
            position: at.to_array(),
            quarter_turns: 0,
            color: 1,
        },
    )
    .await?
    {
        Reply::Planted(id) => Ok(id),
        other => bail!("expected a plant, got {other:?}"),
    }
}

/// Hold `equip`'s item, face `target` and click once.
async fn use_on(
    screens: &mut [&mut Screen; 2],
    who: usize,
    equip: Command,
    target: Vec3,
) -> Result<()> {
    let owner = screens[who].view().owner;
    command(screens, who, equip).await?;
    until(screens, "the item ready", Duration::from_secs(5), |s| {
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
        command(screens, who, Command::WeaponTrigger { down }).await?;
    }
    Ok(())
}

/// Seconds a destroyed brick's pieces stay visible, solid, and in the way.
#[derive(Debug, Clone, Copy)]
struct Feel {
    visible: f32,
    colliding: f32,
    in_the_way: f32,
}

/// Step one kill's debris on this screen's mirror at 60 fps.
fn feel(screen: &Screen, cue: &Cue) -> Result<Feel> {
    let mut debris = BrickDebris::new();
    ensure!(
        debris.cues([cue], &screen.building)? == 1,
        "no debris thrown"
    );
    let spot = Vec3::from(cue.position);
    let mut feel = Feel {
        visible: 0.0,
        colliding: 0.0,
        in_the_way: 0.0,
    };
    let mut t = 0.0;
    while t < 12.0 {
        debris.advance(FRAME, &screen.building)?;
        t += FRAME;
        let seen: Vec<Vec3> = debris
            .instances()
            .filter(|(_, s)| s.tint[3] > SEEN)
            .map(|(_, s)| s.transform.w_axis.truncate())
            .collect();
        if !seen.is_empty() {
            feel.visible = t;
        }
        if !debris.is_empty() {
            feel.colliding = t;
        }
        // Below the ground it is out of sight.
        if seen
            .iter()
            .any(|p| p.distance(spot) < IN_THE_WAY && p.y > -0.3)
        {
            feel.in_the_way = t;
        }
    }
    Ok(feel)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the converted stock catalog and native weapons pack; ~10 s of real time"]
async fn tool_kills_fall_through_the_world_and_blasts_tumble_on_every_screen() -> Result<()> {
    let server = host()?;
    let (address, certificate) = (server.address, server.certificate.clone());
    let client = Client::connect(address, &certificate, "Host".into(), Vec::new(), None).await?;
    let mut host = Screen::open(
        "host",
        Connected {
            client,
            host: Some(server),
            mods: Default::default(),
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
        },
    )
    .await?;
    let screens = &mut [&mut host, &mut joiner];
    until(screens, "both players", Duration::from_secs(5), |s| {
        let view = s.view();
        view.poses.len() >= 2 && view.poses[&view.owner].player.grounded
    })
    .await?;

    // A brick in hammer reach of the host, one in wand reach of the joiner,
    // and one down range of the host for the rocket.
    let (host_feet, joiner_feet) = (screens[0].feet(), screens[1].feet());
    let at = |feet: Vec3, ahead: f32| Vec3::new(feet.x, 0.3, feet.z - ahead);
    let mut spots = [at(host_feet, 2.5), at(joiner_feet, 2.5), Vec3::ZERO];
    let hammered = plant(screens, 0, spots[0]).await?;
    let wanded = plant(screens, 1, spots[1]).await?;

    let killed = |id: BrickId| move |s: &Screen| s.kill(id).is_some();
    use_on(screens, 0, Command::EquipTool { slot: Some(0) }, spots[0]).await?;
    until(
        screens,
        "the hammer kill",
        Duration::from_secs(5),
        killed(hammered),
    )
    .await?;
    use_on(screens, 1, Command::Wand, spots[1]).await?;
    until(
        screens,
        "the wand kill",
        Duration::from_secs(5),
        killed(wanded),
    )
    .await?;
    // Knocked out by a rocket only happens in a Brick Damage minigame.
    let settings = bri_minigames::Settings {
        brick_damage: true,
        ..Default::default()
    };
    command(
        screens,
        0,
        Command::MiniGame(MiniGameRequest::Create { color: 0, settings }),
    )
    .await?;
    // Starting a minigame respawns its owner; the target goes down range
    // of wherever that was.
    tokio::time::sleep(Duration::from_millis(500)).await;
    until(screens, "the host standing", Duration::from_secs(5), |s| {
        s.view().poses.values().all(|p| p.player.grounded)
    })
    .await?;
    spots[2] = at(screens[0].feet(), 10.0);
    let rocketed = plant(screens, 0, spots[2]).await?;
    let owner = screens[0].view().owner;
    let rocket = screens[0].view().tools[&owner]
        .slots
        .iter()
        .position(|s| s.as_deref() == Some(ROCKET))
        .context("rocket in the minigame loadout")?;
    use_on(
        screens,
        0,
        Command::EquipTool { slot: Some(rocket) },
        spots[2],
    )
    .await?;
    until(
        screens,
        "the rocket kill",
        Duration::from_secs(10),
        killed(rocketed),
    )
    .await?;

    until(screens, "all three kills", Duration::from_secs(10), |s| {
        let world = s.view().world;
        [hammered, wanded, rocketed]
            .iter()
            .all(|id| s.kill(*id).is_some())
            && !world.bricks.contains_key(&hammered)
            && !world.bricks.contains_key(&wanded)
            && world.bricks.get(&rocketed).is_some_and(|b| !b.colliding)
    })
    .await?;

    for screen in screens.iter() {
        for (what, id) in [("hammer", hammered), ("wand", wanded), ("rocket", rocketed)] {
            let cue = screen.kill(id).expect("checked above");
            let CueKind::BrickKill { death, .. } = &cue.kind else {
                unreachable!()
            };
            let feel = feel(screen, cue)?;
            println!(
                "{} {what}: visible {:.2} s, colliding {:.2} s, in the way {:.2} s",
                screen.name, feel.visible, feel.colliding, feel.in_the_way
            );
            if what == "rocket" {
                ensure!(*death == BrickDeath::Blast, "{what}: {death:?}");
                // Blasts stay physics debris that tumbles on the ground.
                ensure!(feel.colliding > 1.0, "{}: {what} {feel:?}", screen.name);
            } else {
                ensure!(*death == BrickDeath::Kill, "{what}: {death:?}");
                // v20: never solid, out of the way within a second, gone
                // to the eye within about two and a half.
                ensure!(feel.colliding == 0.0, "{}: {what} {feel:?}", screen.name);
                ensure!(feel.in_the_way < 0.75, "{}: {what} {feel:?}", screen.name);
                ensure!(feel.visible < 2.5, "{}: {what} {feel:?}", screen.name);
            }
        }
    }
    for screen in screens.iter_mut() {
        if let Some(task) = screen.worker.finish() {
            task.await?;
        }
    }
    Ok(())
}
