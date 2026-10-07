//! Add-On client code in a game session: the sandboxed WebAssembly and
//! shaders enabled Add-Ons carry (`docs/architecture/client-sandbox.md`).
//!
//! Code runs only while a game is entered, and only what the game runs
//! and the player trusts. The host decides which Add-Ons a game runs: a
//! joiner runs exactly the host's list of `shared` Add-Ons, downloaded when
//! missing, so what everyone sees in the world (a ragdoll, a beam) looks
//! the same for everyone. Each player keeps their own personal (`client`)
//! Add-Ons, which only ever draw on their screen, wherever they play
//! (`bri_package::library::CodeOwner`). In a game this player hosts, all of
//! it runs. On someone else's server, code the player installed on this PC
//! (or byte for byte the same code) runs without asking, and what
//! `addon-trust.json` grants covers exactly the rest. Everything else is
//! listed and skipped, and the player is asked ([`ClientCode::trust_prompt`])
//! before any of it runs. An Add-On that breaks a budget is stopped with
//! one message; the game carries on.
use bri_client_sandbox::{
    AddOn, AddOnCode, Budgets, FrameInput, Sandbox, Tier, TrustDecision, TrustLevel, TrustPrompt,
    TrustStore,
    gpu::{Camera, GpuSpeed, LayerRenderer},
    host::Frame,
    trust::{CodeSummary, TRUST_FILE},
};
use bri_console::Clamp;
use bri_package::packages::{PackageSet, Side};
use std::path::Path;
use std::sync::Arc;

/// The download cache under the content root, where a server's Add-Ons go.
pub(crate) const DOWNLOADS: &str = ".downloads/";

struct Running {
    addon: AddOn,
    renderer: Option<LayerRenderer>,
    frame: Frame,
    /// Its bodies (`physics.local`), simulated here only.
    physics: crate::addon_physics::AddOnPhysics,
    /// Its slot, which its body handles carry: its code's place in the
    /// loaded list.
    slot: u32,
    /// Its sound files, decoded when it started.
    sounds: std::collections::BTreeMap<String, Arc<bri_audio::SoundAsset>>,
}

/// A sound an Add-On asked for: the clip, where it plays (`None` at the
/// player's ears) and its volume.
pub type AddOnSound = (Arc<bri_audio::SoundAsset>, Option<[f32; 3]>, f32);

/// Who runs the game being entered, for the trust decision.
pub enum Host<'a> {
    /// This player hosts it; their enabled Add-Ons are their own choice.
    Local,
    /// Someone else's server, by its identity key
    /// ([`crate::network::host_trust_key`]). An empty key is a host whose
    /// identity is unknown, which nothing is trusted for.
    Remote(&'a str),
}

/// Where an Add-On's code came from, which decides when it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    /// The player's own personal (`client`) Add-On: it runs wherever they
    /// play.
    Personal,
    /// A `shared` Add-On the player installed on this PC, or code byte for
    /// byte the same as one they installed: it runs, without asking, in a
    /// game whose host runs it.
    Installed,
    /// A `shared` Add-On a server sent, not installed here: it runs once
    /// the player trusts that server's code.
    Sent,
    /// A host's personal Add-On, downloaded with the rest: the host's own
    /// choice for their screen, never run here.
    HostsOwn,
}

#[derive(Default)]
pub struct ClientCode {
    sandbox: Option<Sandbox>,
    /// The package list this code was loaded from.
    set: Option<PackageSet>,
    code: Vec<AddOnCode>,
    /// Per `code`: where it came from.
    origin: Vec<Origin>,
    running: Vec<Running>,
    started: bool,
    /// Whether this game's sandboxed shaders may run: the player's own
    /// game, or a server whose code they trusted.
    trusted: bool,
    time: f32,
    last: Option<f64>,
    messages: Vec<String>,
    /// This device's measured speed, once calibrated (`Some(None)` when
    /// calibration failed and the low default cap stays).
    speed: Option<Option<GpuSpeed>>,
    /// The trust question on screen for the server entered, and that
    /// server's name when it was asked.
    asking: Option<(Box<TrustPrompt>, String)>,
    /// Sounds the last frames asked for, for the game to play.
    sounds: Vec<AddOnSound>,
    /// The player's view as the last frame ran with it.
    view: bri_client_sandbox::View,
    /// Players' bodies as the last frames posed them (`avatar.pose`), for
    /// the next frame's drawing.
    poses: std::collections::BTreeMap<u64, Vec<PosedNode>>,
}

/// A node Add-On code placed: index, world position, world rotation.
pub use bri_client_sandbox::bodies::PosedNode;

impl ClientCode {
    /// Check the client code of every shared and client package in `set`
    /// (a game's package list under `root`). Problems are messages, not
    /// errors: the base game still runs.
    pub fn load(root: &Path, set: &PackageSet) -> Self {
        let mut out = Self {
            set: Some(set.clone()),
            ..Self::default()
        };
        // What the player installed, read only when a server sent code.
        let mut library = None;
        for entry in &set.packages {
            // Base game packages (listed with a role) never carry code.
            if entry.side == Side::Server || entry.role.is_some() {
                continue;
            }
            match AddOnCode::load(&root.join(&entry.dir)) {
                Ok(Some(code)) => {
                    let sent = entry.dir.starts_with(DOWNLOADS);
                    let origin = match (entry.side, sent) {
                        (Side::Client, false) => Origin::Personal,
                        (Side::Client, true) => Origin::HostsOwn,
                        (_, false) => Origin::Installed,
                        (_, true) => {
                            let library = library.get_or_insert_with(|| {
                                bri_package::library::Library::scan(root).ok()
                            });
                            if installed_here(root, library.as_ref(), &code) {
                                Origin::Installed
                            } else {
                                Origin::Sent
                            }
                        }
                    };
                    out.code.push(code);
                    out.origin.push(origin);
                }
                Ok(None) => {}
                Err(problems) => {
                    for p in problems {
                        bri_console::warn(format!(
                            "Add-On {} has code that cannot run: {} ({})",
                            entry.id, p.message, p.code
                        ));
                    }
                }
            }
        }
        out
    }

    /// The package list this code was loaded from.
    pub fn loaded_from(&self) -> Option<&PackageSet> {
        self.set.as_ref()
    }

    /// Whether the code in `slot` runs on someone else's server without
    /// asking: the player's own, as sandboxed code.
    fn runs_unasked(&self, slot: usize) -> bool {
        matches!(self.origin[slot], Origin::Personal | Origin::Installed)
            && CodeSummary::from(&self.code[slot]).tier() == Tier::Sandboxed
    }

    /// Add-Ons with code, loaded and checked.
    pub fn code(&self) -> &[AddOnCode] {
        &self.code
    }

    pub fn is_started(&self) -> bool {
        self.started
    }

    /// Whether the server's sandboxed shaders may run here: the player
    /// hosts, or trusted the server's code (an item's skin, `looks.json`,
    /// is WGSL like an Add-On's code and is held to the same trust).
    pub fn trusts_server(&self) -> bool {
        self.started && self.trusted
    }

    /// Start the code the player trusts, when a game is entered.
    pub fn start(&mut self, host: Host<'_>, state_dir: &Path) {
        self.stop();
        self.started = true;
        self.trusted = matches!(host, Host::Local);
        self.time = 0.0;
        self.last = None;
        if self.code.is_empty() {
            return;
        }
        let trust = match host {
            Host::Local => None,
            Host::Remote(_) => Some(TrustStore::load(state_dir).unwrap_or_else(|e| {
                bri_console::warn(format!("Could not read {TRUST_FILE}: {e:#}"));
                TrustStore::default()
            })),
        };
        if self.sandbox.is_none() {
            match Sandbox::new() {
                Ok(sandbox) => self.sandbox = Some(sandbox),
                Err(e) => {
                    bri_console::warn(format!("Add-On code cannot run on this PC: {e:#}"));
                    return;
                }
            }
        }
        for (slot, code) in self.code.iter().enumerate() {
            if self.origin[slot] == Origin::HostsOwn {
                continue;
            }
            let granted = match (&host, &trust) {
                (Host::Local, _) => Some(TrustLevel::Sandboxed),
                _ if self.runs_unasked(slot) => Some(TrustLevel::Sandboxed),
                (Host::Remote(""), _) => None,
                (Host::Remote(server), Some(store)) => {
                    let granted = store.granted(server, &CodeSummary::from(code));
                    self.trusted |= granted.is_some();
                    granted
                }
                (Host::Remote(_), None) => None,
            };
            let Some(granted) = granted else {
                let summary = CodeSummary::from(code);
                // Only what the player can act on goes to their chat.
                if summary.tier() == Tier::Elevated {
                    bri_console::warn(format!(
                        "{}'s code is off: it asks for more than the sandbox allows",
                        code.name
                    ));
                } else {
                    self.messages.push(format!(
                        "{}'s code is off: you have not trusted this server to run it",
                        code.name
                    ));
                }
                continue;
            };
            let sandbox = self.sandbox.as_ref().expect("created above");
            match sandbox.start_in(code, Budgets::default(), granted, slot as u32) {
                Ok(addon) => {
                    let mut sounds = std::collections::BTreeMap::new();
                    for (name, bytes) in &code.sound_files {
                        let extension = name.rsplit('.').next().unwrap_or_default();
                        // Heard fully within 10 units, gone by 90.
                        match bri_audio::SoundAsset::decoded(
                            &format!("{}:{name}", code.id),
                            bytes,
                            extension,
                            10.0,
                            90.0,
                        ) {
                            Ok(asset) => {
                                sounds.insert(name.clone(), Arc::new(asset));
                            }
                            Err(e) => bri_console::warn(format!(
                                "{}: {name} does not play: {e}",
                                code.name
                            )),
                        }
                    }
                    self.running.push(Running {
                        addon,
                        renderer: None,
                        frame: Frame::default(),
                        physics: Default::default(),
                        slot: slot as u32,
                        sounds,
                    })
                }
                Err(reason) => bri_console::warn(format!("{} stopped: {reason}", code.name)),
            }
        }
    }

    /// Sounds Add-Ons asked for since the last call.
    pub fn take_sounds(&mut self) -> Vec<AddOnSound> {
        std::mem::take(&mut self.sounds)
    }

    /// Stop everything, when the game is left.
    pub fn stop(&mut self) {
        self.running.clear();
        self.sounds.clear();
        self.poses.clear();
        self.started = false;
        self.trusted = false;
        self.asking = None;
    }

    /// On someone else's server (`server`, its identity key, named `name`),
    /// the question to ask when some of its Add-Ons' code is not trusted
    /// yet. Only sandboxed code is asked about: nothing elevated runs in
    /// this build, so its code stays off with a line in chat instead of
    /// asking for full trust that would grant nothing.
    pub fn trust_prompt(
        &mut self,
        server: &str,
        name: &str,
        state_dir: &Path,
    ) -> Option<Box<TrustPrompt>> {
        self.asking = None;
        if server.is_empty() || self.code.is_empty() {
            return None;
        }
        let store = TrustStore::load(state_dir).unwrap_or_default();
        // The player's own code runs without asking; a host's personal
        // Add-Ons never run here.
        let code: Vec<CodeSummary> = (0..self.code.len())
            .filter(|&slot| !self.runs_unasked(slot) && self.origin[slot] != Origin::HostsOwn)
            .map(|slot| CodeSummary::from(&self.code[slot]))
            .collect();
        if code.is_empty() {
            return None;
        }
        let TrustDecision::Ask(prompt) = store.decide(server, name, &code) else {
            return None;
        };
        if prompt.level != TrustLevel::Sandboxed {
            // `start` already named them in chat.
            return None;
        }
        self.asking = Some((prompt.clone(), name.to_string()));
        Some(prompt)
    }

    /// The player chose the trust question's accept: remember exactly what
    /// it showed and start that code.
    pub fn accept_trust(&mut self, state_dir: &Path) -> anyhow::Result<()> {
        let Some((prompt, name)) = self.asking.take() else {
            return Ok(());
        };
        let mut store = TrustStore::load(state_dir)?;
        store.accept(&prompt, &name);
        store.save(state_dir)?;
        self.start(Host::Remote(&prompt.server), state_dir);
        Ok(())
    }

    /// Stop trusting every server's Add-On code (the Add-Ons screen's
    /// Forget Trust). Code already running keeps running until the game is
    /// left; the next join asks again.
    pub fn forget_trust(state_dir: &Path) -> anyhow::Result<()> {
        match std::fs::remove_file(state_dir.join(TRUST_FILE)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    /// Whether any running Add-On declares `capability`.
    fn declared(&self, capability: bri_client_sandbox::Capability) -> bool {
        self.running.iter().any(|r| {
            self.code
                .iter()
                .any(|c| c.id == r.addon.id && c.capabilities.contains(&capability))
        })
    }
    /// Whether any running Add-On reads the world (`world.read`, or players'
    /// bodies for `avatar.pose`), so the game builds a
    /// [`bri_client_sandbox::World`] only when one does.
    pub fn reads_world(&self) -> bool {
        self.declared(bri_client_sandbox::Capability::WorldRead) || self.poses_bodies()
    }
    /// Whether any running Add-On poses players' bodies (`avatar.pose`), so
    /// the world carries their skeletons.
    pub fn poses_bodies(&self) -> bool {
        self.declared(bri_client_sandbox::Capability::AvatarPose)
    }
    /// How Add-On code posed `player`'s body in its last frame, if it did.
    pub fn pose(&self, player: u64) -> Option<&[PosedNode]> {
        self.poses.get(&player).map(Vec::as_slice)
    }
    /// Whether any Add-On has bodies to simulate, so the game gathers
    /// pushers and shots only then.
    pub fn has_bodies(&self) -> bool {
        self.running.iter().any(|r| !r.physics.is_empty())
    }
    /// Simulate every Add-On's bodies for `dt` seconds against the world
    /// this client has, pushed by what it draws. An Add-On whose bodies
    /// took longer than its budget has its oldest moving bodies settled
    /// ([`crate::addon_physics::AddOnPhysics::settle_oldest`]); it is never
    /// stopped for it.
    pub fn advance_physics(
        &mut self,
        dt: f32,
        building: &crate::building::Building,
        pushers: &[crate::local_physics::Pusher],
        shots: &[crate::local_physics::Shot],
    ) -> anyhow::Result<()> {
        let mut result = Ok(());
        for r in &mut self.running {
            let started = std::time::Instant::now();
            if let Err(e) = r.physics.advance(dt, building, pushers, shots) {
                result = Err(e);
                continue;
            }
            let ms = started.elapsed().as_secs_f32() * 1000.0;
            if ms > r.addon.budgets().physics_ms_per_frame {
                r.physics.settle_oldest();
            }
        }
        result
    }

    /// Run every Add-On's `frame` for the frame rendered at `now` (seconds
    /// on any steady clock).
    pub fn run_frame(
        &mut self,
        now: f64,
        eye: glam::Vec3,
        forward: glam::Vec3,
        world: Arc<bri_client_sandbox::World>,
        view: bri_client_sandbox::View,
    ) {
        self.view = view;
        let dt = self
            .last
            .map_or(0.0, |last| (now - last).clamped(0.0, 0.25) as f32);
        self.last = Some(now);
        self.time += dt;
        let sounds = &mut self.sounds;
        let poses = &mut self.poses;
        poses.clear();
        // Every Add-On's shared bodies, which any may find, push and hold.
        let shared: std::collections::BTreeMap<_, _> = self
            .running
            .iter()
            .flat_map(|r| {
                r.physics
                    .snapshot()
                    .iter()
                    .filter(|(_, b)| b.shared)
                    .map(|(id, b)| (*id, *b))
                    .collect::<Vec<_>>()
            })
            .collect();
        let shared = Arc::new(shared);
        // Requests for another Add-On's bodies, applied once all have run.
        let mut elsewhere = Vec::new();
        self.running.retain_mut(|r| {
            let input = FrameInput {
                time: self.time,
                dt,
                eye: eye.to_array(),
                forward: forward.to_array(),
                world: world.clone(),
                view,
                bodies: r.physics.snapshot(),
                shared: shared.clone(),
                ..Default::default()
            };
            let name = r.addon.name.clone();
            match r.addon.frame(input) {
                Ok(frame) => {
                    for line in &frame.log {
                        bri_console::echo(format!("{name}: {line}"));
                    }
                    for sound in &frame.sounds {
                        if let Some(asset) = r.sounds.get(&sound.name)
                            && sounds.len() < 64
                        {
                            sounds.push((asset.clone(), sound.at, sound.volume));
                        }
                    }
                    let (own, others): (Vec<_>, Vec<_>) = frame.physics.iter().partition(|c| {
                        c.body().is_none_or(|b| {
                            bri_client_sandbox::bodies::body_slot(b) == Some(r.slot)
                        })
                    });
                    r.physics.apply(&own);
                    elsewhere.extend(others);
                    for pose in &frame.poses {
                        poses
                            .entry(pose.player)
                            .or_default()
                            .extend(pose.nodes.iter().copied());
                    }
                    r.frame = frame.clone();
                    true
                }
                Err(reason) => {
                    bri_console::warn(format!("{} stopped: {reason}", r.addon.name));
                    false
                }
            }
        });
        for command in elsewhere {
            let slot = command
                .body()
                .and_then(bri_client_sandbox::bodies::body_slot);
            if let Some(r) = self.running.iter_mut().find(|r| Some(r.slot) == slot)
                && command.body().is_some_and(|b| r.physics.shares(b))
            {
                r.physics.apply(&[command]);
            }
        }
    }

    /// Upload what is new and this frame's uniforms. Renderers are built
    /// lazily for the pass's formats.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color: wgpu::TextureFormat,
        depth: wgpu::TextureFormat,
        samples: u32,
        view_projection: glam::Mat4,
        eye: glam::Vec3,
        size: [u32; 2],
    ) {
        if self.running.is_empty() {
            return;
        }
        let speed = *self.speed.get_or_insert_with(|| {
            let speed = bri_client_sandbox::gpu::calibrate(device, queue);
            match speed {
                Some(speed) => bri_console::echo(format!(
                    "Add-On code: GPU measured at {:.2e} shader operations per ms",
                    speed.work_per_ms
                )),
                None => bri_console::warn(
                    "Add-On code: could not measure the GPU; shader loops stay at the low default",
                ),
            }
            speed
        });
        let time = self.time;
        self.running.retain_mut(|r| {
            let renderer = r.renderer.get_or_insert_with(|| {
                LayerRenderer::new(
                    device,
                    queue,
                    speed,
                    color,
                    Some(depth),
                    samples,
                    Budgets::default().draws_per_frame,
                )
            });
            let camera = Camera {
                view_proj: view_projection,
                position: eye,
                size,
                normal_fov: self.view.normal_fov,
            };
            match renderer.prepare(device, queue, &mut r.addon, &r.frame, camera, [time, 0.0]) {
                Ok(()) => true,
                Err(reason) => {
                    bri_console::warn(format!("{} stopped: {reason}", r.addon.name));
                    false
                }
            }
        });
    }

    /// Every running Add-On's world space from another view (a mirror's),
    /// after [`Self::prepare`].
    pub fn prepare_view(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: usize,
        view_projection: glam::Mat4,
        eye: glam::Vec3,
    ) {
        for r in &mut self.running {
            if let Some(renderer) = &mut r.renderer {
                renderer.prepare_view(device, queue, view, view_projection, eye);
            }
        }
    }
    /// [`Self::render`] from a view [`Self::prepare_view`] prepared: world
    /// space only.
    pub fn render_view(&self, pass: &mut wgpu::RenderPass<'_>, view: usize) {
        for r in &self.running {
            if let Some(renderer) = &r.renderer {
                renderer.draw_view(pass, &r.frame, r.addon.layer(), view);
            }
        }
    }
    /// Draw every running Add-On's layer into the world pass.
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        for r in &self.running {
            if let Some(renderer) = &r.renderer {
                renderer.draw(pass, &r.frame, r.addon.layer());
            }
        }
    }

    /// The device went away or the pass changed shape: rebuild renderers.
    /// After the pass `render` drew into ends, before it is submitted.
    pub fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        for r in &self.running {
            if let Some(renderer) = &r.renderer {
                renderer.resolve(encoder);
            }
        }
    }

    pub fn gpu_stopped(&mut self) {
        self.speed = None;
        for r in &mut self.running {
            r.renderer = None;
        }
    }

    /// The graphics card reset. Add-On shaders may have caused it, so every
    /// Add-On's code stops until the next join; the game rebuilds its
    /// renderer as usual.
    pub fn device_lost(&mut self) {
        for r in self.running.drain(..) {
            bri_console::warn(format!(
                "{} stopped: the graphics card reset, so Add-On code is off until you rejoin",
                r.addon.name
            ));
        }
    }

    /// Lines for the player since the last call.
    pub fn take_messages(&mut self) -> Vec<String> {
        std::mem::take(&mut self.messages)
    }

    /// Names of the Add-Ons whose code is running.
    pub fn running(&self) -> Vec<&str> {
        self.running.iter().map(|r| r.addon.name.as_str()).collect()
    }
}

/// Players' bodies as this client draws them, for [`world_view`].
#[derive(Default)]
pub struct DrawnBodies {
    /// Each body's nodes, when an Add-On poses bodies.
    pub skeletons: std::collections::BTreeMap<u64, bri_client_sandbox::world::Skeleton>,
    /// Each drawn body's spawn tick and whether it lies dead
    /// (`avatar::drawn_life`).
    pub lives: std::collections::BTreeMap<u64, (u64, bool)>,
}

/// What the game shows this frame, for Add-On code that reads the world:
/// players and vehicles where they are drawn, the public Add-On state the
/// player receives, and the scene's lighting.
#[allow(clippy::too_many_arguments)]
pub fn world_view(
    view: &crate::network::View,
    entities: &std::collections::BTreeMap<u64, bri_sim::session::EntityInfo>,
    players: &std::collections::BTreeMap<bri_world::OwnerId, bri_sim::player::PlayerState>,
    vehicles: &crate::vehicles::ClientVehicles,
    assets: &crate::vehicles::VehicleAssets,
    camera: &bri_render::scene::Camera,
    items: &crate::world_items::WorldItems,
    image_meshes: std::collections::BTreeMap<
        String,
        std::sync::Arc<bri_client_sandbox::host::Mesh>,
    >,
    drawn: DrawnBodies,
    passages: std::sync::Arc<bri_content::passage::Passages>,
) -> bri_client_sandbox::World {
    let DrawnBodies { skeletons, lives } = drawn;
    use bri_client_sandbox::world::{AddOnState, Entity, Environment, Player, Vehicle, World};
    let players = players
        .iter()
        .map(|(owner, state)| Player {
            id: *owner,
            // Which body is drawn, and whether it lies dead, at the drawn
            // pose's tick (`avatar::drawn_life`); the newest vitals before
            // a body is drawn.
            alive: lives.get(owner).map_or_else(
                || view.vitals.get(owner).is_none_or(|v| v.alive),
                |(_, dead)| !dead,
            ),
            feet: state.feet,
            eye: view.archetypes.eye(state).to_array(),
            look: state.forward().to_array(),
            velocity: state.velocity,
            crouched: state.crouched,
            archetype: view.archetypes.resolve(state.archetype).id.clone(),
            image: view
                .weapons
                .images
                .get(owner)
                .and_then(|images| images.iter().find(|m| m.hand == 0))
                .map(|m| m.image.clone())
                .unwrap_or_default(),
            held: items.held_images(*owner),
            life: lives.get(owner).map_or_else(
                || view.vitals.get(owner).map_or(0, |v| v.spawn_tick),
                |(body, _)| *body,
            ),
        })
        .collect();
    let vehicles = view
        .vehicles
        .values()
        .filter(|info| !info.destroyed)
        .filter_map(|info| {
            let frame = vehicles.frame(info.id)?;
            let radius = assets.definition(&info.definition).map_or(1.0, |d| {
                (glam::Vec3::from(d.bounds_max) - glam::Vec3::from(d.bounds_min)).length() * 0.5
            });
            Some(Vehicle {
                id: info.id,
                definition: info.definition.clone(),
                position: frame.position.to_array(),
                rotation: frame.rotation.to_array(),
                velocity: frame.velocity.to_array(),
                radius,
            })
        })
        .collect();
    let state = view
        .package_state
        .packages
        .iter()
        .map(|(id, ns)| {
            (
                id.clone(),
                AddOnState {
                    global: ns.global.clone(),
                    players: ns.players.clone(),
                },
            )
        })
        .collect();
    let entities = entities
        .values()
        .map(|e| Entity {
            id: e.id,
            kind: e.kind.clone(),
            feet: e.position,
            yaw: e.yaw,
        })
        .collect();
    let rgb = |v: [f32; 4]| [v[0], v[1], v[2]];
    World {
        local: view.owner,
        players,
        vehicles,
        entities,
        state,
        environment: Environment {
            sun_direction: rgb(camera.sun_direction),
            sun_color: rgb(camera.sun_color),
            ambient: rgb(camera.ambient),
            sky: rgb(camera.fog_color),
        },
        image_meshes,
        skeletons,
        passages,
    }
}

/// Whether `code`, which a server sent, is byte for byte the code of an
/// Add-On of the same id the player installed under `root` (turned on or
/// not): code they already run when they host needs no trust to run on a
/// server.
fn installed_here(
    root: &Path,
    library: Option<&bri_package::library::Library>,
    code: &AddOnCode,
) -> bool {
    let Some(entry) = library.and_then(|l| l.get(&code.id)) else {
        return false;
    };
    !entry.package.dir.starts_with(DOWNLOADS)
        && matches!(
            AddOnCode::load(&root.join(&entry.package.dir)),
            Ok(Some(installed)) if installed.code_hash == code.code_hash
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_package::packages::PackageEntry;

    const HOST: &str = "host-key:00112233445566778899aabbccddeeff";

    /// The Spinning Cube as a server sent it (`shared`, in the download
    /// cache, installed nowhere else), whose code needs the player's trust
    /// on someone else's server.
    fn sample_set() -> (std::path::PathBuf, PackageSet) {
        let root = scratch_root();
        let set = copy_sample(&root, ".downloads/ab12", Side::Shared);
        (root, set)
    }
    /// The Spinning Cube installed in the repository's packages, on `side`.
    fn sample_set_on(side: Side) -> (std::path::PathBuf, PackageSet) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
        (root, one("samples/spinning-cube", side))
    }
    fn one(dir: &str, side: Side) -> PackageSet {
        PackageSet {
            schema_version: 1,
            packages: vec![PackageEntry {
                id: "spinning-cube".into(),
                version: "1.0.0".into(),
                side,
                dir: dir.into(),
                role: None,
            }],
        }
    }
    /// A fresh content root of its own for each test.
    fn scratch_root() -> std::path::PathBuf {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("bri-client-code-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }
    /// Copy the Spinning Cube to `dir` under `root`: the set listing it
    /// there on `side`.
    fn copy_sample(root: &Path, dir: &str, side: Side) -> PackageSet {
        let from =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/samples/spinning-cube");
        let to = root.join(dir);
        for file in ["package.json", "client/main.wasm", "client/cube.wgsl"] {
            std::fs::create_dir_all(to.join(file).parent().unwrap()).unwrap();
            std::fs::copy(from.join(file), to.join(file)).unwrap();
        }
        one(dir, side)
    }

    #[test]
    fn a_hosted_game_runs_the_players_own_addon_code() {
        let (root, set) = sample_set();
        let mut code = ClientCode::load(&root, &set);
        assert_eq!(code.code().len(), 1, "{:?}", code.take_messages());
        let state = tempfile::tempdir().unwrap();
        code.start(Host::Local, state.path());
        assert_eq!(code.running(), ["Spinning Cube"]);
        code.run_frame(
            0.0,
            glam::Vec3::new(10.0, 2.0, 5.0),
            glam::Vec3::X,
            Default::default(),
            Default::default(),
        );
        let placed = code.running[0].frame.draws[0].model;
        // Three units ahead of where the camera was.
        assert_eq!([placed[12], placed[14]], [13.0, 5.0]);
        // Its log goes to the console, not the players' chat.
        assert!(code.take_messages().is_empty());
        code.stop();
        assert!(code.running().is_empty());
    }

    #[test]
    fn base_packages_and_data_only_folders_report_nothing() {
        let root = tempfile::tempdir().unwrap();
        // A base package (listed with a role) and a plain data folder:
        // neither has a package.json.
        std::fs::create_dir_all(root.path().join("base/v20-map-bundle")).unwrap();
        std::fs::create_dir_all(root.path().join("some-bricks")).unwrap();
        let entry = |id: &str, dir: &str, role: Option<&str>| PackageEntry {
            id: id.into(),
            version: "1.0.0".into(),
            side: Side::Shared,
            dir: dir.into(),
            role: role.map(str::to_string),
        };
        let set = PackageSet {
            schema_version: 1,
            packages: vec![
                entry("v20-map-bundle", "base/v20-map-bundle", Some("maps")),
                entry("some-bricks", "some-bricks", None),
            ],
        };
        let mut code = ClientCode::load(root.path(), &set);
        assert!(code.code().is_empty());
        assert_eq!(code.take_messages(), Vec::<String>::new());
    }

    #[test]
    fn a_graphics_card_reset_stops_all_addon_code() {
        let (root, set) = sample_set();
        let mut code = ClientCode::load(&root, &set);
        let state = tempfile::tempdir().unwrap();
        code.start(Host::Local, state.path());
        code.take_messages();
        code.device_lost();
        code.gpu_stopped();
        assert!(code.running().is_empty());
        // Why goes to the console, never the players' chat.
        assert!(code.take_messages().is_empty());
        // Frames and draws carry on as for a server with no code.
        code.run_frame(
            0.0,
            glam::Vec3::ZERO,
            glam::Vec3::X,
            Default::default(),
            Default::default(),
        );
        assert!(code.is_started());
    }

    #[test]
    fn a_server_with_no_client_code_changes_nothing() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
        let empty = PackageSet {
            schema_version: 1,
            packages: Vec::new(),
        };
        let mut code = ClientCode::load(&root, &empty);
        let state = tempfile::tempdir().unwrap();
        code.start(Host::Remote(HOST), state.path());
        code.run_frame(
            0.0,
            glam::Vec3::ZERO,
            glam::Vec3::X,
            Default::default(),
            Default::default(),
        );
        assert!(code.running().is_empty());
        assert!(code.take_messages().is_empty());
        // Nothing was calibrated or written.
        assert!(code.speed.is_none());
        assert!(!state.path().join(TRUST_FILE).exists());
    }

    #[test]
    fn someone_elses_server_runs_only_code_the_player_trusted() {
        let (root, set) = sample_set();
        let mut code = ClientCode::load(&root, &set);
        let state = tempfile::tempdir().unwrap();
        code.start(Host::Remote(HOST), state.path());
        assert!(code.running().is_empty());
        assert!(code.take_messages()[0].contains("not trusted this server"));

        let mut store = TrustStore::default();
        let summary = CodeSummary::from(&code.code()[0]);
        let bri_client_sandbox::TrustDecision::Ask(prompt) = store.decide(HOST, "Test", &[summary])
        else {
            panic!()
        };
        store.accept(&prompt, "Test");
        store.save(state.path()).unwrap();
        code.start(Host::Remote(HOST), state.path());
        assert_eq!(code.running(), ["Spinning Cube"]);

        // Another host, or one whose identity is unknown, gets nothing from
        // that grant.
        code.start(
            Host::Remote("host-key:ffeeddccbbaa99887766554433221100"),
            state.path(),
        );
        assert!(code.running().is_empty());
        code.start(Host::Remote(""), state.path());
        assert!(code.running().is_empty());
    }

    #[test]
    fn the_players_own_client_add_on_runs_on_any_server_without_asking() {
        let (root, set) = sample_set_on(Side::Client);
        let mut code = ClientCode::load(&root, &set);
        let state = tempfile::tempdir().unwrap();
        code.start(Host::Remote(HOST), state.path());
        assert_eq!(code.running(), ["Spinning Cube"]);
        assert!(
            code.trust_prompt(HOST, "Brick Town", state.path())
                .is_none()
        );
        code.start(Host::Remote(""), state.path());
        assert_eq!(code.running(), ["Spinning Cube"]);
        assert!(!state.path().join(TRUST_FILE).exists());
    }

    #[test]
    fn code_the_player_installed_runs_where_the_host_runs_it_without_asking() {
        // The host runs it and the player has it on too.
        let (root, set) = sample_set_on(Side::Shared);
        let mut code = ClientCode::load(&root, &set);
        let state = tempfile::tempdir().unwrap();
        code.start(Host::Remote(HOST), state.path());
        assert_eq!(code.running(), ["Spinning Cube"]);
        assert!(
            code.trust_prompt(HOST, "Brick Town", state.path())
                .is_none()
        );

        // The host sent it, and the player has the same code installed but
        // turned off (a default Add-On they never turned on).
        let root = scratch_root();
        copy_sample(&root, "addons/spinning-cube", Side::Shared);
        let sent = copy_sample(&root, ".downloads/ab12", Side::Shared);
        let mut code = ClientCode::load(&root, &sent);
        code.start(Host::Remote(HOST), state.path());
        assert_eq!(code.running(), ["Spinning Cube"]);
        assert!(
            code.trust_prompt(HOST, "Brick Town", state.path())
                .is_none()
        );

        // Different code under the same id is the server's, and asks.
        let shader = root.join(".downloads/ab12/client/cube.wgsl");
        let mut text = std::fs::read_to_string(&shader).unwrap();
        text.push_str("\n// changed by the server\n");
        std::fs::write(&shader, text).unwrap();
        let mut code = ClientCode::load(&root, &sent);
        code.start(Host::Remote(HOST), state.path());
        assert!(code.running().is_empty());
        assert!(
            code.trust_prompt(HOST, "Brick Town", state.path())
                .is_some()
        );
        assert!(!state.path().join(TRUST_FILE).exists());
    }

    #[test]
    fn a_hosts_personal_add_on_never_runs_on_a_joiners_screen() {
        let root = scratch_root();
        let set = copy_sample(&root, ".downloads/ab12", Side::Client);
        let mut code = ClientCode::load(&root, &set);
        let state = tempfile::tempdir().unwrap();
        code.start(Host::Remote(HOST), state.path());
        assert!(code.running().is_empty());
        assert!(code.take_messages().is_empty());
        assert!(
            code.trust_prompt(HOST, "Brick Town", state.path())
                .is_none()
        );
        assert_eq!(code.loaded_from(), Some(&set));
    }

    #[test]
    fn joining_asks_once_and_trust_and_join_starts_the_code() {
        let (root, set) = sample_set();
        let mut code = ClientCode::load(&root, &set);
        let state = tempfile::tempdir().unwrap();
        code.start(Host::Remote(HOST), state.path());
        let prompt = code.trust_prompt(HOST, "Brick Town", state.path()).unwrap();
        assert_eq!((prompt.accept, prompt.decline), ("Trust and join", "Leave"));
        assert_eq!(prompt.rows[0].name, "Spinning Cube");
        assert!(code.running().is_empty());

        code.accept_trust(state.path()).unwrap();
        assert_eq!(code.running(), ["Spinning Cube"]);
        let store = TrustStore::load(state.path()).unwrap();
        assert_eq!(store.servers[HOST].name, "Brick Town");
        // Trusted as is: the next join does not ask.
        code.start(Host::Remote(HOST), state.path());
        assert!(
            code.trust_prompt(HOST, "Brick Town", state.path())
                .is_none()
        );
        assert_eq!(code.running(), ["Spinning Cube"]);

        // Leaving drops an unanswered question; accepting it later does
        // nothing.
        ClientCode::forget_trust(state.path()).unwrap();
        code.start(Host::Remote(HOST), state.path());
        assert!(
            code.trust_prompt(HOST, "Brick Town", state.path())
                .is_some()
        );
        code.stop();
        code.accept_trust(state.path()).unwrap();
        assert!(code.running().is_empty());
        assert!(!state.path().join(TRUST_FILE).exists());
        // Forgetting twice, or with nothing saved, is fine.
        ClientCode::forget_trust(state.path()).unwrap();
    }

    #[test]
    fn a_host_without_an_identity_or_code_asks_nothing() {
        let (root, set) = sample_set();
        let mut code = ClientCode::load(&root, &set);
        let state = tempfile::tempdir().unwrap();
        assert!(code.trust_prompt("", "Anyone", state.path()).is_none());
        let mut none = ClientCode::default();
        assert!(none.trust_prompt(HOST, "Anyone", state.path()).is_none());
    }

    #[test]
    fn trust_is_keyed_by_the_host_certificate_not_its_address() {
        let key = crate::network::host_trust_key(b"certificate");
        assert_eq!(key.len(), "host-key:".len() + 32);
        assert!(key.starts_with("host-key:"));
        assert_ne!(key, crate::network::host_trust_key(b"another certificate"));
    }
}
