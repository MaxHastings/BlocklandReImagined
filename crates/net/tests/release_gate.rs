//! The v0.3.0 release gate: Max's own session, run end to end against his
//! private Mario and Halo Add-Ons, headless (the host's session stepped
//! directly) and with two real clients over loopback QUIC.
//!
//! > Host a server, spawn a Halo rifle and rocket launcher, blow some stuff
//! > up using them as a Blockhead, then turn myself into Mario, jump around,
//! > spawn a Warthog with a wrench on a vehicle spawn, jump in, drive
//! > around, spawn some Goombas, run them over. My friend connects, and we
//! > try to blow each other up in a mini game.
//!
//! Everything happens in one mini-game Max starts right after hosting, with
//! building on. The friend joins as an ordinary player with an empty Add-On
//! cache and becomes Master Chief from a wrench brick event
//! (`ChangeDatablock`). The Goombas' stand-in is the Playground's Stomper.
//! Every step is an ordinary command or movement a player's client sends:
//! bricks are planted and wrenched, items picked up by walking over them,
//! bodies changed by clicking a brick.
//!
//! The host is built as a real one is (`dedicated::load_packages`) on a
//! made-up content root, with the Add-Ons staged into it for the test's
//! length. The private Add-Ons come from `BRI_PRIVATE_ADDONS` (the folder
//! holding `mario/`, `mario-rules/` and the Halo folders, flat or under
//! `halo/`); without it both tests skip. Nothing private is in the
//! repository.
//!
//! Each step prints PASS or FAIL with what it measured. A failed step that
//! later steps need stops the script there; a failed check that nothing
//! depends on (a sound) is reported and the script goes on. The test fails
//! if any step failed.
mod gate;

use anyhow::{Context, Result, bail, ensure};
use bri_console::Clamp;
use bri_net::{
    client::{Client, ClientEvent, HostPin},
    packages::PackageShelf,
    replica::Replica,
    server::{self, ServerHandle, ServerOptions},
};
use bri_package::{packages::Side, sync::Cache};
use bri_progress::Progress;
use bri_sim::{
    player::{MoveInput, PlayerState},
    presentation::{Cue, CueKind},
    session::{
        ActionAim, Command, MiniGameRequest, MiniGameView, Reply, SeatSince, Session, ToolAction,
        ToolInventory, VehicleInfo, VehiclePose, Vitals,
    },
};
use bri_world::{
    Bricks, ContentRef, EventRow, EventTarget, EventValue, ItemSpawn, OwnerId,
    authority::WrenchProperties,
};
use gate::*;
use glam::Vec3;
use std::{collections::BTreeMap, time::Duration};

const MARIO: &str = "mario:archetype/mario";
const SPARTAN: &str = "halo-spartan:archetype/spartan";
const RIFLE: &str = "halo-assault-rifle:weapon/assault_rifle";
const ROCKETS: &str = "halo-rocket-launcher:weapon/rocket_launcher";
const WARTHOG: &str = "halo-warthog:vehicle/warthog";
const STOMPER: &str = "playground-stomper:bot/stomper";
const STOMPER_BODY: &str = "playground-stomper:archetype/stomper";
/// The made-up root's bricks: a 2x2 brick (three plates) and a 2x2 plate.
const BRICK: &str = "fixture/brick/2x2";
const PLATE: &str = "fixture/brick/2x2f";
const SMALL_BRICK: &str = "fixture/brick/1x1";
/// Six by six, a plate high: Warthogs and bots stand on it.
const VEHICLE_SPAWN: &str = bri_sim::testing::VEHICLE_SPAWN;
/// Mario's controller's action byte (`controller-src`): a single jump.
const MARIO_JUMP: u8 = 20;

/// The Add-Ons Max turns on, from the private folder: Mario (both
/// packages) and the six Halo ones in the playtest set; the Grunt, the
/// Elite and the plasma guns are parked and left out.
fn private_set(private: &std::path::Path) -> Result<Vec<AddOn>> {
    let mut add_ons = Vec::new();
    for (id, side) in [
        ("mario", Side::Shared),
        ("mario-rules", Side::Server),
        ("halo-spartan", Side::Shared),
        ("halo-assault-rifle", Side::Shared),
        ("halo-rocket-launcher", Side::Shared),
        ("halo-warthog", Side::Shared),
        ("halo-rules", Side::Server),
        ("halo-mode", Side::Server),
    ] {
        add_ons.push(AddOn::new(id, find_private(private, id)?, side));
    }
    // The Goombas' stand-in, its look and its rules, as the Playground
    // ships them.
    add_ons.extend(
        playground()
            .into_iter()
            .filter(|a| a.id.starts_with("playground-stomper")),
    );
    Ok(add_ons)
}

/// The packages a clean client downloads to play: every shared and client
/// Add-On; the rules and the mode stay on the host.
const DOWNLOADED: [&str; 7] = [
    "halo-assault-rifle",
    "halo-rocket-launcher",
    "halo-spartan",
    "halo-warthog",
    "mario",
    "playground-stomper",
    "playground-stomper-look",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Who {
    Max,
    Friend,
}

/// What a test sees: the host's own session, or a player's replica of it.
enum View<'a> {
    Host(&'a Session),
    Replica(&'a Replica),
}
impl View<'_> {
    fn player(&self, owner: OwnerId) -> Option<PlayerState> {
        match self {
            View::Host(s) => s
                .motion_states()
                .into_iter()
                .find(|(p, _)| p.owner == owner)
                .map(|(p, _)| p),
            View::Replica(r) => r.poses.get(&owner).map(|p| p.player.clone()),
        }
    }
    fn archetypes(&self) -> &bri_sim::archetype::Archetypes {
        match self {
            View::Host(s) => s.archetypes(),
            View::Replica(r) => &r.archetypes,
        }
    }
    fn body(&self, owner: OwnerId) -> String {
        self.player(owner)
            .map(|p| self.archetypes().resolve(p.archetype).id.clone())
            .unwrap_or_default()
    }
    fn feet(&self, owner: OwnerId) -> Vec3 {
        self.player(owner).map_or(Vec3::NAN, |p| Vec3::from(p.feet))
    }
    fn eye(&self, owner: OwnerId) -> Vec3 {
        self.player(owner)
            .map_or(Vec3::NAN, |p| self.archetypes().eye(&p))
    }
    fn vitals(&self, owner: OwnerId) -> Option<Vitals> {
        match self {
            View::Host(s) => s.vitals().remove(&owner),
            View::Replica(r) => r.vitals.get(&owner).cloned(),
        }
    }
    fn alive(&self, owner: OwnerId) -> bool {
        self.vitals(owner).is_some_and(|v| v.alive)
    }
    fn inventory(&self, owner: OwnerId) -> ToolInventory {
        match self {
            View::Host(s) => s.tool_inventories().remove(&owner),
            View::Replica(r) => r.tools.get(&owner).cloned(),
        }
        .unwrap_or_default()
    }
    fn carries(&self, owner: OwnerId, item: &str) -> bool {
        self.inventory(owner)
            .slots
            .iter()
            .any(|s| s.as_deref() == Some(item))
    }
    fn bricks(&self) -> Bricks {
        match self {
            View::Host(s) => s.simulation().state().bricks.clone(),
            View::Replica(r) => r.world.bricks.clone(),
        }
    }
    /// Bricks standing: a knocked-out brick stays in the world, unseen and
    /// passable, until it comes back.
    fn standing(&self, ids: &[u64]) -> usize {
        let bricks = self.bricks();
        ids.iter()
            .filter(|id| {
                bricks
                    .get(*id)
                    .is_some_and(|b| b.visible && b.colliding && b.raycast)
            })
            .count()
    }
    fn vehicles(&self) -> Vec<VehicleInfo> {
        match self {
            View::Host(s) => s.vehicle_infos(),
            View::Replica(r) => r.vehicles.values().cloned().collect(),
        }
    }
    fn vehicle_pose(&self, id: u64) -> Option<VehiclePose> {
        match self {
            View::Host(s) => s.vehicle_poses().into_iter().find(|v| v.id == id),
            View::Replica(r) => r.vehicle_poses.get(&id).cloned(),
        }
    }
    fn names(&self) -> BTreeMap<OwnerId, String> {
        match self {
            View::Host(s) => s.names(),
            View::Replica(r) => r.names.clone(),
        }
    }
    fn minigames(&self) -> Vec<MiniGameView> {
        match self {
            View::Host(s) => s.minigame_views(),
            View::Replica(r) => r.minigames.clone(),
        }
    }
    fn weapons(&self) -> bri_sim::session::WeaponView {
        match self {
            View::Host(s) => s.weapon_view(),
            View::Replica(r) => r.weapons.clone(),
        }
    }
    fn image_state(&self, owner: OwnerId) -> String {
        self.weapons()
            .images
            .get(&owner)
            .and_then(|i| i.first())
            .map(|i| i.state.clone())
            .unwrap_or_default()
    }
}

/// The sounds a cue plays: an archetype's own jump, pain and death cries,
/// a weapon or move sound, a vehicle's.
fn sounds(cue: &Cue) -> Vec<String> {
    match &cue.kind {
        CueKind::Jump { sound } | CueKind::Pain { sound, .. } | CueKind::Death { sound, .. } => {
            sound.iter().cloned().collect()
        }
        CueKind::WeaponSound { profile } => vec![profile.clone()],
        CueKind::VehicleSound { sound, .. } => vec![sound.clone()],
        _ => vec![],
    }
}

/// How the script plays: on the host's session directly, or through two
/// real clients.
trait Driver {
    fn owner(&self, who: Who) -> OwnerId;
    /// What `viewer` sees now.
    fn view(&self, viewer: Who) -> View<'_>;
    /// A command from `who`'s client, aimed where `aim` says (a click),
    /// and the host's reply.
    fn command(&mut self, who: Who, command: Command, aim: Option<ActionAim>) -> Result<Reply>;
    /// `who` holds `input` for `ticks` of their own moves, then lets go of
    /// the keys, keeping the look.
    fn hold(&mut self, who: Who, input: MoveInput, ticks: usize) -> Result<()> {
        self.keep(who, input, ticks)?;
        self.keep(who, released(input), 1)
    }
    /// `who` holds `input` for `ticks` and keeps holding it (a driver's
    /// throttle between corrections).
    fn keep(&mut self, who: Who, input: MoveInput, ticks: usize) -> Result<()>;
    /// The game runs on for `ticks`.
    fn wait(&mut self, ticks: u64) -> Result<()>;
    /// Every cue `viewer` has been sent so far.
    fn cues(&self, viewer: Who) -> &[Cue];
    /// The friend connects; returns the Add-Ons a clean cache downloaded
    /// (None on the host's own session).
    fn friend_connects(&mut self) -> Result<Option<Vec<String>>>;
    /// The look `who` last sent.
    fn look(&self, who: Who) -> MoveInput;
    fn package_problems(&self) -> Vec<String>;
}

/// The headless host: the session, stepped here, every player moving.
struct Local {
    s: Session,
    spawn: Vec3,
    owners: BTreeMap<Who, OwnerId>,
    seq: BTreeMap<OwnerId, u64>,
    moves: BTreeMap<OwnerId, u64>,
    looks: BTreeMap<OwnerId, MoveInput>,
    cues: Vec<Cue>,
}
impl Local {
    fn new(host: bri_net::dedicated::Dedicated) -> Result<Self> {
        let mut s = host.session;
        let spawn = host.spawn_points[0];
        let max = s.join("Max".into(), spawn, true)?;
        let mut me = Self {
            s,
            spawn,
            owners: [(Who::Max, max)].into(),
            seq: BTreeMap::new(),
            moves: BTreeMap::new(),
            looks: [(max, MoveInput::default())].into(),
            cues: Vec::new(),
        };
        me.wait(120)?;
        Ok(me)
    }
    fn step(&mut self) -> Result<()> {
        for (owner, input) in &self.looks {
            let m = self.moves.entry(*owner).or_default();
            *m += 1;
            let _ = self.s.movement(*owner, *m, *input);
        }
        self.s.step()?;
        self.cues.extend(self.s.take_cues());
        Ok(())
    }
}
impl Driver for Local {
    fn owner(&self, who: Who) -> OwnerId {
        self.owners[&who]
    }
    fn view(&self, _: Who) -> View<'_> {
        View::Host(&self.s)
    }
    fn command(&mut self, who: Who, command: Command, aim: Option<ActionAim>) -> Result<Reply> {
        let owner = self.owners[&who];
        let n = self.seq.entry(owner).or_default();
        *n += 1;
        self.s.command_with_aim(owner, *n, command, aim)
    }
    fn hold(&mut self, who: Who, input: MoveInput, ticks: usize) -> Result<()> {
        self.keep(who, input, ticks)?;
        self.looks.insert(self.owners[&who], released(input));
        Ok(())
    }
    fn keep(&mut self, who: Who, input: MoveInput, ticks: usize) -> Result<()> {
        let owner = self.owners[&who];
        self.looks.insert(owner, input);
        for _ in 0..ticks {
            self.step()?;
        }
        Ok(())
    }
    fn wait(&mut self, ticks: u64) -> Result<()> {
        for _ in 0..ticks {
            self.step()?;
        }
        Ok(())
    }
    fn cues(&self, _: Who) -> &[Cue] {
        &self.cues
    }
    fn friend_connects(&mut self) -> Result<Option<Vec<String>>> {
        let friend = self.s.join("Friend".into(), self.spawn, false)?;
        self.owners.insert(Who::Friend, friend);
        self.looks.insert(friend, MoveInput::default());
        Ok(None)
    }
    fn look(&self, who: Who) -> MoveInput {
        self.looks[&self.owners[&who]]
    }
    fn package_problems(&self) -> Vec<String> {
        self.s
            .package_diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect()
    }
}

/// Keys up, look kept.
fn released(input: MoveInput) -> MoveInput {
    MoveInput {
        yaw: input.yaw,
        pitch: input.pitch,
        head_yaw: input.head_yaw,
        ..Default::default()
    }
}

/// One player's real client.
struct Player {
    client: Client,
    seat: Option<SeatSince>,
    /// The newest move sent: the next batch follows it.
    sent: u64,
    look: MoveInput,
    cues: Vec<Cue>,
    _cache: tempfile::TempDir,
}
impl Player {
    fn take_cues(&mut self) {
        self.cues.extend(self.client.replica.take_cues());
    }
}

/// The host over real QUIC, Max and his friend each on a client of their
/// own. Max hosts: his client carries the host's token and has every
/// Add-On; the friend starts from an empty cache.
struct Remote {
    rt: tokio::runtime::Runtime,
    server: Option<ServerHandle>,
    players: BTreeMap<Who, Player>,
    base: Vec<bri_package::environment::PackageRef>,
    all: Vec<bri_package::environment::PackageRef>,
    identity_dir: tempfile::TempDir,
}

const LINK: Duration = Duration::from_secs(30);

async fn receive_until(
    player: &mut Player,
    mut done: impl FnMut(&Client, &ClientEvent) -> bool,
) -> Result<()> {
    tokio::time::timeout(LINK, async {
        loop {
            let event = player.client.receive().await?;
            player.cues.extend(player.client.replica.take_cues());
            if done(&player.client, &event) {
                return Result::<()>::Ok(());
            }
        }
    })
    .await
    .context("the host stopped answering")?
}

async fn catch_up(player: &mut Player, tick: u64) -> Result<()> {
    if player.client.replica.tick >= tick {
        return Ok(());
    }
    receive_until(player, |c, _| c.replica.tick >= tick)
        .await
        .with_context(|| format!("waiting for tick {tick}"))
}

impl Remote {
    fn new(root: &Root, host: bri_net::dedicated::Dedicated) -> Result<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()?;
        let base = root.base_packages(&host);
        let all = host.environment.packages.clone();
        let shelf = PackageShelf::new(root.path(), &root.set, &host.environment)?;
        let options = ServerOptions {
            bind: "127.0.0.1:0".parse()?,
            environment: host.environment.clone(),
            spawn_points: host.spawn_points.clone(),
            certificate: None,
            map_loader: Some(host.setup.clone()),
            packages: Some(std::sync::Arc::new(shelf)),
        };
        let server = {
            let _in = rt.enter();
            server::start(host.session, options)?
        };
        let mut me = Self {
            rt,
            server: Some(server),
            players: BTreeMap::new(),
            base,
            all,
            identity_dir: tempfile::tempdir()?,
        };
        // Max hosts: his own client has every Add-On and the host's token.
        me.connect(Who::Max)?;
        me.wait(120)?;
        Ok(me)
    }
    /// Connects `who` with an empty cache; returns what it downloaded.
    fn connect(&mut self, who: Who) -> Result<Vec<String>> {
        let server = self.server.as_ref().unwrap();
        let (address, pin) = (server.address, HostPin::from(&server.certificate[..]));
        let (have, host, name) = match who {
            Who::Max => (self.all.clone(), Some(server.host_token.clone()), "Max"),
            Who::Friend => (self.base.clone(), None, "Friend"),
        };
        let identity = bri_identity::ClientIdentity::load_or_create(
            self.identity_dir.path().join(format!("{name}.identity")),
        )?;
        let cache_dir = tempfile::tempdir()?;
        let cache = Cache::open(cache_dir.path())?;
        let (client, fetched, _) = self.rt.block_on(Client::connect_fetching(
            address,
            pin,
            name.into(),
            have.clone(),
            host,
            &identity,
            &cache,
            Progress::default(),
            |fetched, _| {
                // What the game does with a download: load it as the
                // client's own Add-Ons, then join declaring it.
                let dirs: Vec<_> = fetched
                    .iter()
                    .map(|f| {
                        (
                            f.dir.clone(),
                            bri_package::packages::PackageEntry {
                                id: f.package.id.clone(),
                                version: f.package.version.clone(),
                                side: f.package.side,
                                dir: f.dir.to_string_lossy().into_owned(),
                                role: None,
                            },
                        )
                    })
                    .collect();
                let (_, problems) = bri_package_runtime::Catalog::load_dirs_skipping(&dirs, false);
                ensure!(
                    problems.is_empty(),
                    "a downloaded Add-On does not load: {problems:?}"
                );
                Ok(have
                    .iter()
                    .cloned()
                    .chain(fetched.iter().map(|f| f.package.clone()))
                    .collect())
            },
        ))?;
        let mut ids: Vec<String> = fetched.iter().map(|f| f.package.id.clone()).collect();
        ids.sort();
        self.players.insert(
            who,
            Player {
                client,
                seat: None,
                sent: 0,
                look: MoveInput::default(),
                cues: Vec::new(),
                _cache: cache_dir,
            },
        );
        Ok(ids)
    }
    /// Every client reads up to the newest tick any has seen.
    fn sync(&mut self) -> Result<()> {
        let newest = self
            .players
            .values()
            .map(|p| p.client.replica.tick)
            .max()
            .unwrap_or(0);
        let players = &mut self.players;
        self.rt.block_on(async {
            for player in players.values_mut() {
                catch_up(player, newest).await?;
                player.take_cues();
            }
            Result::<()>::Ok(())
        })
    }
}
impl Drop for Remote {
    fn drop(&mut self) {
        self.players.clear();
        if let Some(server) = self.server.take() {
            let _ = self.rt.block_on(server.stop());
        }
    }
}
impl Driver for Remote {
    fn owner(&self, who: Who) -> OwnerId {
        self.players[&who].client.owner
    }
    fn view(&self, viewer: Who) -> View<'_> {
        View::Replica(&self.players[&viewer].client.replica)
    }
    fn command(&mut self, who: Who, command: Command, aim: Option<ActionAim>) -> Result<Reply> {
        let player = self.players.get_mut(&who).unwrap();
        let reply = self.rt.block_on(async {
            let sequence = player.client.request_with_aim(command, aim).await?;
            let mut reply = None;
            receive_until(player, |_, event| match event {
                ClientEvent::Reply {
                    sequence: s,
                    result,
                } if *s == sequence => {
                    reply = Some(result.clone());
                    true
                }
                _ => false,
            })
            .await?;
            Result::<_>::Ok(reply.unwrap())
        })?;
        self.sync()?;
        reply.map_err(|r| anyhow::anyhow!("{r:?}"))
    }
    fn keep(&mut self, who: Who, input: MoveInput, ticks: usize) -> Result<()> {
        let player = self.players.get_mut(&who).unwrap();
        player.look = released(input);
        self.rt.block_on(async {
            // As a real client does: a batch of moves goes out while the
            // last is still queued at the host, so the host never runs a
            // tick without the held keys.
            let mut left = ticks;
            let mut sent: Option<u64> = None;
            while left > 0 {
                let n = left.min(bri_net::protocol::MOVEMENT_REDUNDANCY);
                let owner = player.client.owner;
                let acknowledged = player.client.replica.poses[&owner].acknowledged_input;
                let first = player.sent.max(acknowledged) + 1;
                let newest = first + n as u64 - 1;
                player.sent = newest;
                player.seat =
                    SeatSince::follow(player.seat, player.client.replica.seat_of(owner), first);
                player
                    .client
                    .movement(newest, &vec![input; n], None, player.seat)?;
                let previous = sent.unwrap_or(newest);
                sent = Some(newest);
                left -= n;
                // Wait for the batch before this one: one stays queued. A
                // move the host never ran (it drops what is queued when a
                // rider leaves a seat, and a datagram can be lost) goes
                // again as a new move, as a real client's next frame's
                // moves would.
                let mut until = if left == 0 { newest } else { previous };
                let mut tries = 0;
                loop {
                    let acked = |c: &Client| {
                        c.replica
                            .poses
                            .get(&c.owner)
                            .is_some_and(|p| p.acknowledged_input >= until)
                    };
                    if acked(&player.client) {
                        break;
                    }
                    let wait = receive_until(player, |c, _| acked(c));
                    if tokio::time::timeout(Duration::from_millis(250), wait)
                        .await
                        .is_ok()
                    {
                        continue;
                    }
                    tries += 1;
                    let pose = &player.client.replica.poses[&owner];
                    let acknowledged = pose.acknowledged_input;
                    ensure!(
                        tries < 60,
                        "move {until} never acknowledged (at {acknowledged}, seat {:?}, tick {})",
                        player.seat,
                        player.client.replica.tick,
                    );
                    let again = (until.saturating_sub(acknowledged) as usize)
                        .clamp(1, bri_net::protocol::MOVEMENT_REDUNDANCY);
                    let first = player.sent.max(acknowledged) + 1;
                    let newest = first + again as u64 - 1;
                    player.sent = newest;
                    player.seat =
                        SeatSince::follow(player.seat, player.client.replica.seat_of(owner), first);
                    player
                        .client
                        .movement(newest, &vec![input; again], None, player.seat)?;
                    until = newest;
                }
            }
            Result::<()>::Ok(())
        })?;
        self.sync()
    }
    fn wait(&mut self, ticks: u64) -> Result<()> {
        let target = self
            .players
            .values()
            .map(|p| p.client.replica.tick)
            .max()
            .unwrap_or(0)
            + ticks;
        let players = &mut self.players;
        self.rt.block_on(async {
            for player in players.values_mut() {
                catch_up(player, target).await?;
            }
            Result::<()>::Ok(())
        })?;
        self.sync()
    }
    fn cues(&self, viewer: Who) -> &[Cue] {
        &self.players[&viewer].cues
    }
    fn friend_connects(&mut self) -> Result<Option<Vec<String>>> {
        let ids = self.connect(Who::Friend)?;
        self.wait(120)?;
        Ok(Some(ids))
    }
    fn look(&self, who: Who) -> MoveInput {
        self.players[&who].look
    }
    fn package_problems(&self) -> Vec<String> {
        self.server
            .as_ref()
            .map(|s| {
                s.package_problems
                    .lock()
                    .unwrap()
                    .iter()
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------
// What a player does, through any driver.

/// The yaw and pitch from `from` to `to`, as a look and a click's aim.
fn aim(from: Vec3, to: Vec3) -> ActionAim {
    let d = to - from;
    ActionAim {
        yaw: d.x.atan2(-d.z),
        pitch: d.y.atan2(Vec3::new(d.x, 0.0, d.z).length()),
    }
}

/// `who` turns to look at `point`, and the aim to click it with.
fn look_at(d: &mut dyn Driver, who: Who, point: Vec3) -> Result<ActionAim> {
    let at = aim(d.view(who).eye(d.owner(who)), point);
    d.hold(
        who,
        MoveInput {
            yaw: at.yaw,
            pitch: at.pitch,
            ..Default::default()
        },
        4,
    )?;
    // Where the eye is now (crouch and steps move it).
    Ok(aim(d.view(who).eye(d.owner(who)), point))
}

/// Walk `who` to `target` (in x and z), within `within`.
fn walk_to(d: &mut dyn Driver, who: Who, target: Vec3, within: f32) -> Result<()> {
    let from = d.view(who).feet(d.owner(who));
    // Where the walk stood five strides (half a second) ago: no closer
    // since means something is in the way (a parked Warthog), so a player
    // steps aside. Mario turns and gets going slowly, so not sooner.
    let mut strides: Vec<f32> = Vec::new();
    let mut side = 1.0;
    for _ in 0..120 {
        let at = d.view(who).feet(d.owner(who));
        let to = Vec3::new(target.x - at.x, 0.0, target.z - at.z);
        if to.length() <= within {
            // Let go of the keys there.
            let look = d.look(who);
            return d.hold(who, released(look), 1);
        }
        strides.push(to.length());
        if to.length() > 3.0 && strides.len() >= 6 && strides[strides.len() - 6] - to.length() < 0.5
        {
            let look = d.look(who);
            d.hold(
                who,
                MoveInput {
                    forward: -0.5,
                    right: side,
                    yaw: to.x.atan2(-to.z),
                    pitch: look.pitch,
                    ..Default::default()
                },
                48,
            )?;
            side = -side;
            strides.clear();
            continue;
        }
        let look = d.look(who);
        // Held between corrections, as a player holds W.
        d.keep(
            who,
            MoveInput {
                forward: 1.0,
                yaw: to.x.atan2(-to.z),
                pitch: look.pitch,
                ..Default::default()
            },
            if to.length() > 3.0 { 12 } else { 4 },
        )?;
    }
    let look = d.look(who);
    d.hold(who, released(look), 1)?;
    let owner = d.owner(who);
    let view = d.view(who);
    let near: Vec<String> = view
        .vehicles()
        .iter()
        .filter_map(|v| {
            let at = Vec3::from(view.vehicle_pose(v.id)?.position);
            (at.distance(view.feet(owner)) < 6.0)
                .then(|| format!("{} {} at {at}", v.definition, v.id))
        })
        .collect();
    let vitals = view.vitals(owner);
    bail!(
        "{who:?} never reached {target} (at {}, from {from}; seated {:?}, alive {:?}, vehicles near {near:?})",
        view.feet(owner),
        vitals.as_ref().map(|v| v.mounted),
        vitals.as_ref().map(|v| v.alive),
    )
}

fn plant(d: &mut dyn Driver, who: Who, definition: &str, position: [f32; 3]) -> Result<u64> {
    match d.command(
        who,
        Command::Plant {
            definition: definition.into(),
            position,
            quarter_turns: 0,
            color: 3,
        },
        None,
    )? {
        Reply::Planted(id) => Ok(id),
        other => bail!("planting at {position:?}: {other:?}"),
    }
}

/// The slot `item` is in, if `who` carries it.
fn slot(d: &dyn Driver, who: Who, item: &str) -> Option<usize> {
    d.view(who)
        .inventory(d.owner(who))
        .slots
        .iter()
        .position(|s| s.as_deref() == Some(item))
}

fn equip(d: &mut dyn Driver, who: Who, item: &str) -> Result<()> {
    let slot = slot(d, who, item).with_context(|| format!("{who:?} carries no {item}"))?;
    d.command(who, Command::EquipTool { slot: Some(slot) }, None)?;
    Ok(())
}

/// Swing the wrench at `brick` until the host opens its wrench dialog
/// (`inspected` says whether a follow-up dialog command takes), as a player
/// clicks again when a swing misses.
fn wrench(
    d: &mut dyn Driver,
    who: Who,
    brick: u64,
    then: impl Fn(&mut dyn Driver) -> Result<()>,
) -> Result<()> {
    equip(d, who, bri_weapons::WRENCH)?;
    d.wait(40)?;
    let at = Vec3::from(d.view(who).bricks()[&brick].position);
    let mut last = None;
    for _ in 0..6 {
        let aim = look_at(d, who, at)?;
        d.command(who, Command::WeaponTrigger { down: true }, Some(aim))?;
        d.wait(12)?;
        d.command(who, Command::WeaponTrigger { down: false }, Some(aim))?;
        d.wait(30)?;
        match then(d) {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap()).context("the wrench dialog never took")
}

fn visible() -> WrenchProperties {
    WrenchProperties {
        raycast: true,
        colliding: true,
        visible: true,
        ..Default::default()
    }
}

/// A brick whose `onActivate` makes the clicking player `body`.
fn class_brick(d: &mut dyn Driver, who: Who, at: [f32; 3], body: &str) -> Result<u64> {
    let id = plant(d, who, BRICK, at)?;
    let rows = vec![EventRow {
        conditions: vec![],
        preserved: None,
        enabled: true,
        input: "onActivate".into(),
        delay_ms: 0,
        target: EventTarget::Slot(bri_events::Slot::Player),
        output: "ChangeDatablock".into(),
        params: vec![EventValue::Datablock(Some(body.into()))],
    }];
    wrench(d, who, id, |d| {
        d.command(
            who,
            Command::Tool(ToolAction::Inspect {
                mode: bri_sim::session::InspectMode::Events,
            }),
            None,
        )?;
        d.command(
            who,
            Command::Tool(ToolAction::SetEvents {
                brick: id,
                events: rows.clone(),
            }),
            None,
        )?;
        Ok(())
    })?;
    Ok(id)
}

/// `who` puts their tool away and clicks `brick`.
fn click(d: &mut dyn Driver, who: Who, brick: u64) -> Result<()> {
    d.command(who, Command::EquipTool { slot: None }, None)?;
    let at = Vec3::from(d.view(who).bricks()[&brick].position);
    walk_to(d, who, at, 3.0)?;
    let aim = look_at(d, who, at)?;
    match d.command(who, Command::Activate, Some(aim))? {
        Reply::Activated(Some(id)) if id == brick => {}
        other => bail!("the click did not reach the brick: {other:?}"),
    }
    d.command(who, Command::ActivateRelease, Some(aim))?;
    d.wait(4)?;
    Ok(())
}

/// The highest `who`'s feet rise in two seconds after a jump from standing,
/// and the controller action bytes seen.
fn jump(d: &mut dyn Driver, who: Who, held: usize) -> Result<(f32, Vec<u8>)> {
    let owner = d.owner(who);
    d.wait(60)?;
    let ground = d.view(who).feet(owner).y;
    let look = d.look(who);
    let mut top = ground;
    let mut actions = Vec::new();
    let mut note = |d: &dyn Driver, top: &mut f32| {
        let p = d.view(who).player(owner).unwrap();
        *top = top.max(p.feet[1]);
        let a = p.controller.map_or(0, |c| c.memory[0]);
        if actions.last() != Some(&a) {
            actions.push(a);
        }
    };
    // The key held down for `held` ticks, as a player presses it.
    for _ in 0..held {
        d.keep(
            who,
            MoveInput {
                jump: true,
                ..released(look)
            },
            1,
        )?;
        note(d, &mut top);
    }
    d.hold(who, released(look), 1)?;
    for _ in 0..60 {
        d.wait(4)?;
        note(d, &mut top);
    }
    Ok((top - ground, actions))
}

/// `who` fires the item in hand once at `point`; returns the shot's id.
fn fire_at(d: &mut dyn Driver, who: Who, point: Vec3) -> Result<Option<u64>> {
    let owner = d.owner(who);
    let before: Vec<u64> = d.view(who).weapons().fired().map(|p| p.id).collect();
    let aim = look_at(d, who, point)?;
    d.command(who, Command::WeaponTrigger { down: true }, Some(aim))?;
    let mut shot = None;
    for _ in 0..8 {
        d.wait(2)?;
        shot = shot.or_else(|| {
            d.view(who)
                .weapons()
                .fired()
                .find(|p| p.source.0 == owner && !before.contains(&p.id))
                .map(|p| p.id)
        });
    }
    d.command(who, Command::WeaponTrigger { down: false }, Some(aim))?;
    Ok(shot)
}

/// Hold the trigger at `point` for `ticks`; returns rounds fired (each
/// entry into a `Fire` state).
fn burst_at(d: &mut dyn Driver, who: Who, point: Vec3, ticks: u64) -> Result<usize> {
    let owner = d.owner(who);
    let aim = look_at(d, who, point)?;
    d.command(who, Command::WeaponTrigger { down: true }, Some(aim))?;
    let mut last = d.view(who).image_state(owner);
    let mut rounds = 0;
    for _ in 0..ticks / 2 {
        d.wait(2)?;
        let now = d.view(who).image_state(owner);
        if now.starts_with("Fire") && now != last {
            rounds += 1;
        }
        last = now;
    }
    d.command(who, Command::WeaponTrigger { down: false }, Some(aim))?;
    Ok(rounds)
}

/// Run at the vehicle from behind, hopping, until seated (v20 boards from
/// above).
fn board(d: &mut dyn Driver, who: Who, vehicle: u64) -> Result<u8> {
    let owner = d.owner(who);
    for _ in 0..12 {
        if who == Who::Max {
            revive(d)?;
        }
        let at = Vec3::from(
            d.view(who)
                .vehicle_pose(vehicle)
                .context("no vehicle")?
                .position,
        );
        let to = at - d.view(who).feet(owner);
        let yaw = to.x.atan2(-to.z);
        if Vec3::new(to.x, 0.0, to.z).length() < 6.0 {
            d.hold(
                who,
                MoveInput {
                    forward: -1.0,
                    yaw,
                    ..Default::default()
                },
                40,
            )?;
        } else {
            walk_to(d, who, at, 7.0)?;
        }
        for i in 0..20 {
            d.hold(
                who,
                MoveInput {
                    forward: 1.0,
                    jump: (2..12).contains(&i),
                    yaw,
                    ..Default::default()
                },
                4,
            )?;
            if let Some((v, seat)) = d.view(who).vitals(owner).and_then(|v| v.mounted)
                && v == vehicle
            {
                return Ok(seat);
            }
        }
    }
    let view = d.view(who);
    let vitals = view.vitals(owner);
    bail!(
        "never got into vehicle {vehicle} (at {:?}; {who:?} at {}, seated {:?}, alive {:?})",
        view.vehicle_pose(vehicle).map(|p| p.position),
        view.feet(owner),
        vitals.as_ref().map(|v| v.mounted),
        vitals.as_ref().map(|v| v.alive),
    )
}

fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// The driver's look for one step toward `target`, steering a
/// mouse-steered vehicle as a player's mouse does (and as the bots'
/// driving closes the loop on the steering angle).
fn steer(pose: &VehiclePose, look: MoveInput, target: Vec3, forward: f32) -> MoveInput {
    let at = Vec3::from(pose.position);
    let ahead = glam::Quat::from_array(pose.rotation) * Vec3::NEG_Z;
    let heading = ahead.x.atan2(-ahead.z);
    let to = target - at;
    let want = to.x.atan2(-to.z);
    let error = wrap(want - heading) * forward.signum();
    let desired = (error * 1.5).clamped(-0.5, 0.5);
    MoveInput {
        forward,
        yaw: wrap(look.yaw + (desired - pose.mouse_steering[0]).clamped(-0.04, 0.04)),
        pitch: look.pitch,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------
// The script.

struct Steps {
    lines: Vec<(String, Result<String, String>)>,
    known: Vec<(String, Result<String, String>)>,
}
impl Steps {
    /// Runs one step; Err stops the script when later steps need it.
    fn run(&mut self, name: &str, f: impl FnOnce() -> Result<String>) -> Result<(), String> {
        let outcome = f().map_err(|e| format!("{e:#}"));
        let shown = match &outcome {
            Ok(evidence) => format!("PASS {name}: {evidence}"),
            Err(why) => format!("FAIL {name}: {why}"),
        };
        eprintln!("[gate] {shown}");
        self.lines.push((name.into(), outcome.clone()));
        outcome.map(drop)
    }
    /// A step whose failure is a known v0.3.1 item (a content gap or an
    /// engine observation that does not block the release): reported as
    /// KNOWN, never failing the gate.
    fn known(&mut self, name: &str, f: impl FnOnce() -> Result<String>) {
        let outcome = f().map_err(|e| format!("{e:#}"));
        match &outcome {
            Ok(evidence) => eprintln!("[gate] PASS {name}: {evidence}"),
            Err(why) => eprintln!("[gate] KNOWN {name}: {why}"),
        }
        self.known.push((name.into(), outcome));
    }
    fn report(&self, label: &str) -> Result<()> {
        eprintln!("\n[gate] {label}:");
        for (name, outcome) in &self.lines {
            match outcome {
                Ok(e) => eprintln!("  PASS  {name}: {e}"),
                Err(e) => eprintln!("  FAIL  {name}: {e}"),
            }
        }
        for (name, outcome) in &self.known {
            match outcome {
                Ok(e) => eprintln!("  PASS  {name}: {e}"),
                Err(e) => eprintln!("  KNOWN {name} (v0.3.1, not blocking): {e}"),
            }
        }
        let failed: Vec<_> = self
            .lines
            .iter()
            .filter(|(_, o)| o.is_err())
            .map(|(n, _)| n.as_str())
            .collect();
        ensure!(failed.is_empty(), "{label}: failed steps {failed:?}");
        Ok(())
    }
}

/// Positions, from the room's spawn at (0, 0, 20) looking down -z: a wall
/// 20 ahead, the item spawns on the way to it, the class bricks either side
/// of the spawn, the vehicle pad off to the left and the Stomper's spawn
/// beyond it.
const WALL_Z: f32 = 0.25;
const RIFLE_SPAWN: [f32; 3] = [0.0, 0.1, 15.0];
const ROCKET_SPAWN: [f32; 3] = [0.0, 0.1, 13.0];
const MARIO_BRICK: [f32; 3] = [-4.0, 0.3, 14.0];
const CHIEF_BRICK: [f32; 3] = [4.0, 0.3, 14.0];
const PAD: [f32; 3] = [-30.0, 0.1, 10.0];
const STOMPER_SPAWN: [f32; 3] = [-30.0, 0.1, -30.0];

fn script(d: &mut dyn Driver, steps: &mut Steps) {
    // A step that failed is already a FAIL line; anything else that
    // stopped the script early fails the gate here, never silently.
    let recorded = |steps: &Steps| steps.lines.iter().filter(|(_, o)| o.is_err()).count();
    let before = recorded(steps);
    if let Err(why) = run(d, steps)
        && recorded(steps) == before
    {
        steps
            .lines
            .push(("the script ran to its end".into(), Err(why)));
    }
}

fn run(d: &mut dyn Driver, steps: &mut Steps) -> Result<(), String> {
    let max = d.owner(Who::Max);
    let mut game = 0;
    steps.run("host: every Add-On loads", || {
        let problems = d.package_problems();
        ensure!(problems.is_empty(), "{problems:?}");
        let view = d.view(Who::Max);
        for body in [MARIO, SPARTAN, STOMPER_BODY] {
            ensure!(view.archetypes().find(body).is_some(), "no {body}");
        }
        Ok(format!("Max joined as {}", view.body(max)))
    })?;
    steps.run("mini-game started right after hosting, building on", || {
        let settings = bri_minigames::Settings {
            title: "Max's game".into(),
            enable_building: true,
            loadout: [
                Some(bri_weapons::HAMMER.into()),
                Some(bri_weapons::WRENCH.into()),
                Some(bri_weapons::PRINTER.into()),
                None,
                None,
            ],
            ..Default::default()
        };
        d.command(
            Who::Max,
            Command::MiniGame(MiniGameRequest::Create { color: 0, settings }),
            None,
        )?;
        d.wait(150)?;
        let view = d.view(Who::Max);
        let g = view.minigames().first().cloned().context("no mini-game")?;
        game = g.id;
        ensure!(g.members.contains(&max), "Max is not in it");
        ensure!(view.alive(max), "Max is not alive");
        Ok(format!(
            "game {} (brick damage {})",
            g.id, g.settings.brick_damage
        ))
    })?;
    // A wall of 1x1 bricks, 8 wide and 4 high, to blow up. (The Halo
    // rocket breaks bricks of at most 4 studs x plates, its `brick`
    // `max_volume`: a 1x1 brick is 3, a 2x2 brick 12.)
    let mut wall = Vec::new();
    steps.run("build a wall", || {
        for row in 0..4 {
            for column in 0..8 {
                wall.push(plant(
                    d,
                    Who::Max,
                    SMALL_BRICK,
                    [column as f32 * 0.5 - 1.75, 0.3 + 0.6 * row as f32, WALL_Z],
                )?);
            }
        }
        d.wait(10)?;
        let standing = d.view(Who::Max).standing(&wall);
        ensure!(standing == 32, "{standing} of 32 stand");
        Ok("32 1x1 bricks".into())
    })?;
    // Steps 1-2: a Halo rifle and rocket launcher from item spawn bricks
    // set with the wrench, picked up by walking over them.
    let blockhead = d.view(Who::Max).body(max);
    steps.run(
        "spawn the Halo rifle and rocket launcher with the wrench",
        || {
            for (at, item) in [(RIFLE_SPAWN, RIFLE), (ROCKET_SPAWN, ROCKETS)] {
                let id = plant(d, Who::Max, PLATE, at)?;
                wrench(d, Who::Max, id, |d| {
                    d.command(
                        Who::Max,
                        Command::Tool(ToolAction::SetWrench {
                            brick: id,
                            properties: WrenchProperties {
                                item_spawn: ItemSpawn {
                                    item: Some(ContentRef::Resolved(item.into())),
                                    position: 0,
                                    direction: 2,
                                    respawn_ms: 4000,
                                    restore: Default::default(),
                                },
                                ..visible()
                            },
                        }),
                        None,
                    )?;
                    Ok(())
                })?;
            }
            d.wait(30)?;
            let shown: Vec<String> = d
                .view(Who::Max)
                .weapons()
                .static_items
                .iter()
                .map(|i| i.item.clone())
                .collect();
            ensure!(
                shown.iter().any(|i| i == RIFLE) && shown.iter().any(|i| i == ROCKETS),
                "the bricks show {shown:?}"
            );
            Ok(format!("item bricks show {shown:?}"))
        },
    )?;
    steps.run("pick both up as a Blockhead", || {
        for at in [RIFLE_SPAWN, ROCKET_SPAWN] {
            walk_to(d, Who::Max, Vec3::from(at), 0.3)?;
            d.wait(20)?;
        }
        let view = d.view(Who::Max);
        ensure!(
            view.carries(max, RIFLE) && view.carries(max, ROCKETS),
            "carries {:?}",
            view.inventory(max).slots
        );
        ensure!(
            view.body(max) == blockhead,
            "not a Blockhead: {}",
            view.body(max)
        );
        Ok(format!(
            "{} carries {:?}",
            blockhead,
            view.inventory(max).slots
        ))
    })?;
    steps.run("blow the wall up with the Halo guns as a Blockhead", || {
        let target = Vec3::new(0.0, 1.2, WALL_Z);
        equip(d, Who::Max, RIFLE)?;
        d.wait(200)?;
        let rounds = burst_at(d, Who::Max, target, 60)?;
        ensure!(rounds > 0, "the rifle never fired");
        equip(d, Who::Max, ROCKETS)?;
        d.wait(200)?;
        let before = d.view(Who::Max).standing(&wall);
        let mut fired = 0;
        let mut after = before;
        for _ in 0..3 {
            if fire_at(d, Who::Max, target)?.is_some() {
                fired += 1;
            }
            d.wait(150)?;
            after = d.view(Who::Max).standing(&wall);
            if after < before {
                break;
            }
        }
        ensure!(fired > 0, "no rocket left the launcher");
        ensure!(
            after < before,
            "{fired} rockets; still {after} of {before} bricks stand"
        );
        Ok(format!(
            "{rounds} rifle rounds in 0.5 s; {fired} rocket(s) knocked {} of {before} bricks out",
            before - after
        ))
    })?;
    // Step 3: Mario, from a brick event.
    let (blockhead_jump, _) = jump(d, Who::Max, 12).map_err(|e| e.to_string())?;
    let mario_brick = steps
        .run("a wrench brick event to turn into Mario", || {
            let id = class_brick(d, Who::Max, MARIO_BRICK, MARIO)?;
            Ok(format!(
                "brick {id}: onActivate -> Player -> ChangeDatablock {MARIO}"
            ))
        })
        .map(|_| ());
    mario_brick?;
    steps.run("click it: Max becomes Mario", || {
        let id = brick_at(d, MARIO_BRICK)?;
        click(d, Who::Max, id)?;
        d.wait(10)?;
        let view = d.view(Who::Max);
        ensure!(view.body(max) == MARIO, "Max is {}", view.body(max));
        // The controller's state may be blank while he stands (as a
        // replica sees it); the jump below shows it running.
        let archetype = view.player(max).map(|p| p.archetype).context("no player")?;
        let carried = view.archetypes().resolve(archetype).controller.is_some();
        ensure!(carried, "Mario's body carries no controller");
        Ok(format!(
            "Max is {MARIO}, his body carrying the Mario controller"
        ))
    })?;
    let cues_before_jumps = d.cues(Who::Max).len();
    steps.run("jump around as Mario: his controller, not the Blockhead's", || {
        let (height, actions) = jump(d, Who::Max, 12)?;
        ensure!(
            actions.contains(&MARIO_JUMP),
            "the controller never took the jump: actions {actions:?}"
        );
        ensure!(
            (height - blockhead_jump).abs() > 0.15,
            "Mario jumped {height:.2}, the Blockhead {blockhead_jump:.2}"
        );
        // A few more hops.
        for _ in 0..3 {
            jump(d, Who::Max, 6)?;
        }
        Ok(format!(
            "Mario jumped {height:.2} (Blockhead {blockhead_jump:.2}); controller actions {actions:?}"
        ))
    })?;
    // Not blocking: whether Mario's own sounds play.
    // A content gap, not an engine bug: the Mario Add-On has no sounds yet
    // (its NOTES: they need `intent_sounds`). Listed, not blocking.
    steps.known("Mario's own sounds (the wahoo) are cued", || {
        let heard: Vec<String> = d.cues(Who::Max)[cues_before_jumps..]
            .iter()
            .flat_map(sounds)
            .collect();
        ensure!(
            heard.iter().any(|s| s.starts_with("mario")),
            "no Mario Add-On sound in {} cues while Mario jumped; sounds heard: {heard:?}",
            d.cues(Who::Max).len() - cues_before_jumps
        );
        Ok(format!("{heard:?}"))
    });
    // Step 4: the Warthog from a vehicle spawn brick.
    let mut hog = 0;
    steps.run(
        "spawn a Warthog with the wrench on a vehicle spawn brick",
        || {
            walk_to(d, Who::Max, Vec3::from(PAD) + Vec3::new(5.0, 0.0, 0.0), 0.5)?;
            let pad = plant(d, Who::Max, VEHICLE_SPAWN, PAD)?;
            wrench(d, Who::Max, pad, |d| {
                d.command(
                    Who::Max,
                    Command::Tool(ToolAction::SetWrench {
                        brick: pad,
                        properties: WrenchProperties {
                            vehicle: Some(WARTHOG.into()),
                            ..visible()
                        },
                    }),
                    None,
                )?;
                Ok(())
            })?;
            d.wait(120)?;
            let view = d.view(Who::Max);
            let v = view
                .vehicles()
                .into_iter()
                .find(|v| v.definition == WARTHOG)
                .context("no Warthog")?;
            hog = v.id;
            let at = Vec3::from(view.vehicle_pose(hog).context("no pose")?.position);
            ensure!(
                Vec3::new(at.x - PAD[0], 0.0, at.z - PAD[2]).length() < 2.0,
                "the Warthog is at {at}, not on its pad"
            );
            Ok(format!("Warthog {hog} on its pad at {at}"))
        },
    )?;
    steps.run("jump in as Mario", || {
        let seat = board(d, Who::Max, hog)?;
        ensure!(seat == 0, "seat {seat}, not the driver's");
        ensure!(d.view(Who::Max).body(max) == MARIO, "no longer Mario");
        Ok("Mario in the driver's seat".into())
    })?;
    steps.run("drive around", || {
        let start = Vec3::from(d.view(Who::Max).vehicle_pose(hog).unwrap().position);
        let mut top = 0.0f32;
        let mut far = 0.0f32;
        // Out toward the far wall, then round.
        for target in [
            Vec3::new(-30.0, 0.0, -40.0),
            Vec3::new(30.0, 0.0, -40.0),
            Vec3::new(30.0, 0.0, 30.0),
        ] {
            for _ in 0..60 {
                let pose = d.view(Who::Max).vehicle_pose(hog).unwrap();
                let at = Vec3::from(pose.position);
                if Vec3::new(target.x - at.x, 0.0, target.z - at.z).length() < 8.0 {
                    break;
                }
                let input = steer(&pose, d.look(Who::Max), target, 1.0);
                d.keep(Who::Max, input, 4)?;
                top = top.max(Vec3::from(pose.velocity).length());
                far = far.max(at.distance(start));
            }
        }
        ensure!(
            far > 30.0 && top > 15.0,
            "went {far:.1} at up to {top:.1} units/s"
        );
        ensure!(
            d.view(Who::Max)
                .vitals(max)
                .and_then(|v| v.mounted)
                .map(|m| m.0)
                == Some(hog),
            "no longer driving"
        );
        Ok(format!(
            "drove {far:.1} from the pad, up to {top:.1} units/s"
        ))
    })?;
    // Step 5: Goombas (Stompers) from a bot spawn brick, run over.
    let mut stomper = 0;
    let pad = Vec3::from(STOMPER_SPAWN) + Vec3::new(0.0, 0.0, 5.0);
    let walked = leave_warthog(d, pad);
    steps.known(
        "leave the Warthog on foot (to set the Stomper brick)",
        || {
            walked.as_ref().map_err(|e| anyhow::anyhow!("{e:#}"))?;
            Ok("off the roof and at the Stomper pad".into())
        },
    );
    if walked.is_err() {
        // Where the reseated Warthog went is anyone's guess: a fresh one
        // on its pad, out of the way, and Max back where he was going.
        steps.run("out by a respawn, a fresh Warthog (Stomper brick)", || {
            respawn_as_mario(d, pad)?;
            hog = fresh_warthog(d, hog)?;
            walk_to(d, Who::Max, pad, 0.5)?;
            Ok(format!(
                "Mario again at the Stomper pad; Warthog {hog} on its pad"
            ))
        })?;
    }
    steps.run("spawn some Goombas (Stompers) from a spawn brick", || {
        let at = d.view(Who::Max).feet(max);
        let spot = [
            (at.x * 2.0).round() / 2.0 - 5.0,
            0.1,
            (at.z * 2.0).round() / 2.0,
        ];
        let brick = plant(d, Who::Max, VEHICLE_SPAWN, spot)?;
        wrench(d, Who::Max, brick, |d| {
            d.command(
                Who::Max,
                Command::Tool(ToolAction::SetWrench {
                    brick,
                    properties: WrenchProperties {
                        vehicle: Some(STOMPER.into()),
                        ..visible()
                    },
                }),
                None,
            )?;
            Ok(())
        })?;
        d.wait(120)?;
        let view = d.view(Who::Max);
        let found: Vec<OwnerId> = view
            .names()
            .keys()
            .copied()
            .filter(|o| *o != max && view.body(*o) == STOMPER_BODY)
            .collect();
        stomper = *found.first().context("no Stomper")?;
        let mg = view.vitals(stomper).and_then(|v| v.minigame);
        ensure!(
            mg == Some(game),
            "the Stomper is in {mg:?}, not Max's game {game}"
        );
        Ok(format!("Stomper {stomper} in Max's mini-game"))
    })?;
    steps.run("run them over in the Warthog", || {
        // A Warthog Max drove off by being seated again (observation (a))
        // can be gone from the room: the pad makes a new one, as a player
        // would ask it to.
        let lost = d.view(Who::Max).vehicle_pose(hog).is_none_or(|p| {
            let at = Vec3::from(p.position);
            at.y < -1.0 || at.x.abs() > HALF || at.z.abs() > HALF
        });
        if lost {
            eprintln!("[gate] the Warthog left the room; the pad makes a new one");
            hog = fresh_warthog(d, hog)?;
        }
        // Back in (run there as Mario), past the spawn protection.
        board(d, Who::Max, hog)?;
        d.wait(300)?;
        for attempt in 0..6 {
            // Away to get up speed, then at it.
            for phase in 0..2 {
                for _ in 0..60 {
                    if !d.view(Who::Max).alive(stomper) {
                        break;
                    }
                    let pose = d.view(Who::Max).vehicle_pose(hog).unwrap();
                    let target = d.view(Who::Max).feet(stomper);
                    let at = Vec3::from(pose.position);
                    let gap = Vec3::new(target.x - at.x, 0.0, target.z - at.z).length();
                    let input = if phase == 0 {
                        if gap > 18.0 {
                            break;
                        }
                        // Drive away from it, staying well inside the room.
                        let away = (at * 2.0 - target).clamp(Vec3::splat(-50.0), Vec3::splat(50.0));
                        steer(&pose, d.look(Who::Max), away, 1.0)
                    } else {
                        steer(&pose, d.look(Who::Max), target, 1.0)
                    };
                    d.keep(Who::Max, input, 4)?;
                }
            }
            if !d.view(Who::Max).alive(stomper) {
                // Park near the spawn, where the friend arrives.
                let home = Vec3::new(4.0, 0.0, 22.0);
                for _ in 0..200 {
                    let pose = d.view(Who::Max).vehicle_pose(hog).context("no Warthog")?;
                    let at = Vec3::from(pose.position);
                    if Vec3::new(home.x - at.x, 0.0, home.z - at.z).length() < 8.0 {
                        break;
                    }
                    let input = steer(&pose, d.look(Who::Max), home, 0.6);
                    d.keep(Who::Max, input, 4)?;
                }
                d.hold(Who::Max, released(d.look(Who::Max)), 90)?;
                return Ok(format!("run over on pass {}", attempt + 1));
            }
        }
        bail!(
            "still alive after six passes: health {:?}",
            d.view(Who::Max).vitals(stomper).map(|v| v.health)
        )
    })?;
    // The friend.
    steps.run(
        "the friend connects; a clean cache downloads every Add-On",
        || {
            let fetched = d.friend_connects()?;
            if let Some(ids) = &fetched {
                ensure!(
                    ids == &DOWNLOADED,
                    "downloaded {ids:?}, wanted {DOWNLOADED:?}"
                );
            }
            let friend = d.owner(Who::Friend);
            let view = d.view(Who::Friend);
            ensure!(
                view.player(friend).is_some(),
                "the friend is not in the world"
            );
            ensure!(
                view.body(max) == MARIO,
                "the friend sees Max as {}",
                view.body(max)
            );
            Ok(match fetched {
                Some(ids) => format!("downloaded {ids:?}; sees Max as Mario"),
                None => "joined (headless: no download)".into(),
            })
        },
    )?;
    let friend = d.owner(Who::Friend);
    steps.run("the friend joins the mini-game", || {
        d.command(
            Who::Friend,
            Command::MiniGame(MiniGameRequest::Join { game }),
            None,
        )?;
        d.wait(150)?;
        let v = d.view(Who::Friend).vitals(friend).context("no vitals")?;
        ensure!(v.minigame == Some(game) && v.alive, "{v:?}");
        Ok("in Max's game".into())
    })?;
    let back = Vec3::from(CHIEF_BRICK) + Vec3::new(0.0, 0.0, 3.0);
    let walked = leave_warthog(d, back);
    steps.known("leave the Warthog on foot (to set the Chief brick)", || {
        walked.as_ref().map_err(|e| anyhow::anyhow!("{e:#}"))?;
        Ok("off the roof and back at the spawn".into())
    });
    if walked.is_err() {
        steps.run("out by a respawn, a fresh Warthog (Chief brick)", || {
            respawn_as_mario(d, back)?;
            let parked = fresh_warthog(d, hog)?;
            walk_to(d, Who::Max, back, 0.5)?;
            Ok(format!(
                "Mario again by the Chief brick; Warthog {parked} on its pad"
            ))
        })?;
    }
    // The remounted Warthog can run the friend over on its way.
    if !d.view(Who::Friend).alive(friend) {
        eprintln!("[gate] the friend was run over by the remounted Warthog; respawning");
        for _ in 0..10 {
            if d.view(Who::Friend).alive(friend) {
                break;
            }
            d.command(Who::Friend, Command::Respawn, None).ok();
            d.wait(100).map_err(|e| e.to_string())?;
        }
    }
    let cues_before_chief = d.cues(Who::Friend).len();
    steps.run(
        "the friend becomes Master Chief from a wrench brick event",
        || {
            let id = class_brick(d, Who::Max, CHIEF_BRICK, SPARTAN)?;
            click(d, Who::Friend, id)?;
            d.wait(10)?;
            let body = d.view(Who::Friend).body(friend);
            ensure!(body == SPARTAN, "the friend is {body}");
            let seen = d.view(Who::Max).body(friend);
            ensure!(seen == SPARTAN, "Max sees the friend as {seen}");
            // His controller's state stays blank while he stands; the next
            // step shows it running. Here: his body carries one.
            let view = d.view(Who::Friend);
            let archetype = view
                .player(friend)
                .map(|p| p.archetype)
                .context("no player")?;
            let carried = view.archetypes().resolve(archetype).controller.is_some();
            ensure!(carried, "Chief's body carries no controller");
            Ok(
                "the friend is Chief, his body carrying the Halo controller; Max sees him so"
                    .into(),
            )
        },
    )?;
    steps.run("Chief moves through his own controller", || {
        let from = d.view(Who::Friend).feet(friend);
        let look = d.look(Who::Friend);
        // His run speed as the host moves him, sampled while W is held.
        let mut speed = 0.0f32;
        for _ in 0..10 {
            d.keep(
                Who::Friend,
                MoveInput {
                    forward: 1.0,
                    ..released(look)
                },
                12,
            )?;
            let v = d
                .view(Who::Friend)
                .player(friend)
                .context("no player")?
                .velocity;
            speed = speed.max(Vec3::new(v[0], 0.0, v[2]).length());
        }
        d.hold(Who::Friend, released(look), 30)?;
        let to = d.view(Who::Friend).feet(friend);
        let ran = Vec3::new(to.x - from.x, 0.0, to.z - from.z).length();
        ensure!(
            (speed - 9.27).abs() < 0.3,
            "ran at up to {speed:.2} units/s (Halo 9.27)"
        );
        // A tap of about a tenth of a second. Found by this gate: standing
        // still, Chief creeps up off the floor (about 0.01 units/s) until
        // the motor no longer counts him grounded, and the Halo controller
        // drops a press made then; a player presses again.
        let mut lost = 0;
        let mut height = 0.0;
        for _ in 0..3 {
            let grounded = d
                .view(Who::Friend)
                .player(friend)
                .is_some_and(|p| p.grounded);
            height = jump(d, Who::Friend, 12)?.0;
            if height > 2.0 {
                break;
            }
            lost += 1;
            eprintln!("[gate] Chief's jump press was lost (grounded before the wait: {grounded})");
        }
        ensure!(
            height > 2.0,
            "Chief jumped {height:.2} (Halo: about his height)"
        );
        Ok(format!(
            "ran at {speed:.2} units/s (Halo 9.27), {ran:.1} units; jumped {height:.2} \
             ({lost} press(es) lost standing still)"
        ))
    })?;
    // The friend arms himself from the same item bricks.
    steps.run("the friend picks up the Halo guns", || {
        d.wait(500)?;
        for at in [RIFLE_SPAWN, ROCKET_SPAWN] {
            walk_to(d, Who::Friend, Vec3::from(at), 0.3)?;
            d.wait(30)?;
        }
        let view = d.view(Who::Friend);
        ensure!(
            view.carries(friend, RIFLE) && view.carries(friend, ROCKETS),
            "carries {:?}",
            view.inventory(friend).slots
        );
        Ok("rifle and launcher".into())
    })?;
    steps.run("blow each other up in the mini-game", || {
        revive(d)?;
        // Apart, facing each other.
        walk_to(d, Who::Friend, Vec3::new(10.0, 0.0, -20.0), 1.0)?;
        walk_to(d, Who::Max, Vec3::new(-10.0, 0.0, -20.0), 1.0)?;
        let chest = |d: &dyn Driver, o: OwnerId| d.view(Who::Max).feet(o) + Vec3::Y * 1.2;
        // Max (Mario) fires the rifle at Chief.
        let max_health = d.view(Who::Max).vitals(max).unwrap().health;
        let friend_health = d.view(Who::Friend).vitals(friend).unwrap().health;
        equip(d, Who::Max, RIFLE)?;
        d.wait(200)?;
        let target = chest(d, friend);
        let rounds = burst_at(d, Who::Max, target, 120)?;
        d.wait(30)?;
        let hit_friend = {
            let v = d.view(Who::Friend).vitals(friend).unwrap();
            !v.alive || v.health < friend_health
        };
        // The shield takes the first 75: read it from the rules' state too.
        let shield = match d.view(Who::Friend) {
            View::Host(s) => shield(&s.package_state(), friend),
            View::Replica(r) => shield(&r.package_state, friend),
        };
        // Chief (if standing) fires a rocket at Mario.
        if !d.view(Who::Friend).alive(friend) {
            d.command(Who::Friend, Command::Respawn, None).ok();
            d.wait(240)?;
        }
        equip(d, Who::Friend, ROCKETS)?;
        d.wait(200)?;
        let target = chest(d, max);
        let rocket = fire_at(d, Who::Friend, target)?;
        d.wait(120)?;
        let v = d.view(Who::Max).vitals(max).unwrap();
        let hit_max = !v.alive || v.health < max_health;
        ensure!(rocket.is_some(), "Chief's rocket never left");
        ensure!(
            (hit_friend || shield < 75.0) && hit_max,
            "Mario fired {rounds} rounds (Chief hit: {hit_friend}, shield {shield}); \
             Chief's rocket hit Mario: {hit_max} (health {}, alive {})",
            v.health,
            v.alive
        );
        Ok(format!(
            "Mario's {rounds} rifle rounds: Chief's shield {shield}, hurt {hit_friend}; \
             Chief's rocket: Mario health {}, alive {}",
            v.health, v.alive
        ))
    })?;
    steps.run("a respawn keeps the chosen bodies", || {
        for who in [Who::Max, Who::Friend] {
            let o = d.owner(who);
            if d.view(who).alive(o) {
                d.command(who, Command::Suicide, None)?;
            }
        }
        d.wait(200)?;
        for who in [Who::Max, Who::Friend] {
            let o = d.owner(who);
            if !d.view(who).alive(o) {
                d.command(who, Command::Respawn, None).ok();
            }
        }
        d.wait(200)?;
        let (m, f) = (d.view(Who::Friend).body(max), d.view(Who::Max).body(friend));
        ensure!(
            m == MARIO && f == SPARTAN,
            "after respawning: Max {m}, friend {f}"
        );
        Ok("Max respawned as Mario, the friend as Chief".into())
    })?;
    let _ = steps.run("Chief's own sounds are cued", || {
        let heard: Vec<String> = d.cues(Who::Friend)[cues_before_chief..]
            .iter()
            .flat_map(sounds)
            .collect();
        let chief: Vec<&String> = heard
            .iter()
            .filter(|s| s.starts_with("halo-spartan"))
            .collect();
        ensure!(!chief.is_empty(), "no halo-spartan sound among {heard:?}");
        Ok(format!("{chief:?}"))
    });
    let _ = steps.run("no Add-On problems", || {
        let problems = d.package_problems();
        ensure!(problems.is_empty(), "{problems:?}");
        Ok("none".into())
    });
    Ok(())
}

fn shield(state: &bri_sim::session::PackageStateView, owner: OwnerId) -> f64 {
    state
        .packages
        .get("halo-rules")
        .and_then(|n| n.players.get(&owner))
        .and_then(|p| p.get("shield"))
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0)
}

/// Max out of the Warthog (jet), off it before v20's MinMountTime lets
/// touching it seat him again, and on foot to `back`.
///
/// Found by this gate: jetting out leaves Mario on the Warthog's roof,
/// where his controller barely moves him, and once MinMountTime has passed
/// touching it seats him again; then this fails and Max gets out by
/// [`respawn_as_mario`], as a player would.
fn leave_warthog(d: &mut dyn Driver, back: Vec3) -> Result<()> {
    let max = d.owner(Who::Max);
    let look = d.look(Who::Max);
    d.hold(
        Who::Max,
        MoveInput {
            jet: true,
            ..released(look)
        },
        2,
    )?;
    d.hold(Who::Max, released(look), 2)?;
    ensure!(
        d.view(Who::Max)
            .vitals(max)
            .and_then(|v| v.mounted)
            .is_none(),
        "the jet did not take him out"
    );
    for _ in 0..12 {
        let at = d.view(Who::Max).feet(max);
        let to = back - at;
        d.hold(
            Who::Max,
            MoveInput {
                forward: 1.0,
                jump: true,
                yaw: to.x.atan2(-to.z),
                ..Default::default()
            },
            6,
        )?;
    }
    walk_to(d, Who::Max, back, 0.5)?;
    ensure!(
        d.view(Who::Max)
            .vitals(max)
            .and_then(|v| v.mounted)
            .is_none(),
        "seated again"
    );
    Ok(())
}

/// Max out by Ctrl+K and a respawn, as Mario again (his brick) with the
/// rifle (its item brick), standing at `back`.
fn respawn_as_mario(d: &mut dyn Driver, back: Vec3) -> Result<()> {
    let max = d.owner(Who::Max);
    if d.view(Who::Max).alive(max) {
        d.command(Who::Max, Command::Suicide, None)?;
    }
    let mut last = None;
    for _ in 0..10 {
        d.wait(100)?;
        if d.view(Who::Max).alive(max) {
            break;
        }
        last = Some(d.command(Who::Max, Command::Respawn, None));
    }
    ensure!(d.view(Who::Max).alive(max), "Max never respawned: {last:?}");
    d.wait(60)?;
    if d.view(Who::Max).body(max) != MARIO {
        let brick = brick_at(d, MARIO_BRICK)?;
        click(d, Who::Max, brick)?;
        d.wait(10)?;
    }
    for _ in 0..4 {
        if d.view(Who::Max).carries(max, RIFLE) {
            break;
        }
        walk_to(d, Who::Max, Vec3::from(RIFLE_SPAWN), 0.3).ok();
        d.wait(150)?;
    }
    ensure!(
        d.view(Who::Max).body(max) == MARIO,
        "Max is {}",
        d.view(Who::Max).body(max)
    );
    ensure!(
        d.view(Who::Max).carries(max, RIFLE),
        "Max never got the rifle back"
    );
    walk_to(d, Who::Max, back, 0.5)
}

/// Max back as Mario if the mini-game's Stompers (or the Warthog) killed
/// him, as a player clicks to respawn.
fn revive(d: &mut dyn Driver) -> Result<()> {
    if !d.view(Who::Max).alive(d.owner(Who::Max)) {
        eprintln!("[gate] Max was killed in the mini-game; respawning as Mario");
        respawn_as_mario(d, Vec3::new(-10.0, 0.0, 21.0))?;
    }
    Ok(())
}

/// A fresh Warthog on its pad by the wrench's `< Respawn >` (the pad
/// already names the Warthog), as a player gets one back; its id.
fn fresh_warthog(d: &mut dyn Driver, old: u64) -> Result<u64> {
    // From the spawn, round behind the class bricks and item spawns.
    let at = d.view(Who::Max).feet(d.owner(Who::Max));
    if at.z > 8.0 && at.x > -8.0 {
        walk_to(d, Who::Max, Vec3::new(-10.0, 0.0, 21.0), 1.0)?;
    }
    walk_to(d, Who::Max, Vec3::from(PAD) + Vec3::new(5.0, 0.0, 0.0), 0.5)?;
    let pad = brick_at(d, PAD)?;
    wrench(d, Who::Max, pad, |d| {
        d.command(
            Who::Max,
            Command::Tool(ToolAction::RespawnVehicle { brick: pad }),
            None,
        )?;
        Ok(())
    })?;
    d.wait(120)?;
    d.view(Who::Max)
        .vehicles()
        .into_iter()
        .filter(|v| v.definition == WARTHOG && v.id != old)
        .map(|v| v.id)
        .max()
        .context("the pad made no new Warthog")
}

/// The brick standing at `at` (as planted).
fn brick_at(d: &dyn Driver, at: [f32; 3]) -> Result<u64> {
    d.view(Who::Max)
        .bricks()
        .iter()
        .find(|(_, b)| Vec3::from(b.position).distance(Vec3::from(at)) < 0.01)
        .map(|(id, _)| *id)
        .context("no brick there")
}

fn host() -> Result<Option<(Root, bri_net::dedicated::Dedicated)>> {
    let Some(private) = private_add_ons() else {
        eprintln!("skipped: BRI_PRIVATE_ADDONS is not set (the private Mario and Halo Add-Ons)");
        return Ok(None);
    };
    let root = Root::new("release-gate", &private_set(&private)?)?;
    let host = root.host(world("Max's server"))?;
    Ok(Some((root, host)))
}

/// Max's session on the host's own session, every step in one game.
#[test]
fn max_session_headless() -> Result<()> {
    let Some((_root, host)) = host()? else {
        return Ok(());
    };
    let mut d = Local::new(host)?;
    let mut steps = Steps {
        lines: Vec::new(),
        known: Vec::new(),
    };
    script(&mut d, &mut steps);
    steps.report("Max's session, headless")
}

/// Max's session with Max and his friend on real clients over loopback
/// QUIC; the friend joins with an empty cache.
#[test]
fn max_session_two_clients() -> Result<()> {
    let Some((root, host)) = host()? else {
        return Ok(());
    };
    let mut d = Remote::new(&root, host)?;
    let mut steps = Steps {
        lines: Vec::new(),
        known: Vec::new(),
    };
    script(&mut d, &mut steps);
    let result = steps.report("Max's session, two clients");
    drop(d);
    result
}
