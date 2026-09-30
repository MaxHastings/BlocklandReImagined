//! Add-On client code in a game session: the sandboxed WebAssembly and
//! shaders enabled Add-Ons carry (`docs/architecture/client-sandbox.md`).
//!
//! Code runs only while a game is entered, and only what the player
//! trusts: in a game this player hosts, their own enabled Add-Ons; on
//! someone else's server, their own enabled `client` Add-Ons (which only
//! ever draw on their screen, like the Ragdoll) and what `addon-trust.json`
//! grants for exactly the rest of the code. Everything else is listed and skipped, and the player is asked
//! ([`ClientCode::trust_prompt`]) before any of it runs. An Add-On that
//! breaks a budget is stopped with one message; the game carries on.
use bri_client_sandbox::{
    AddOn, AddOnCode, Budgets, FrameInput, Sandbox, Tier, TrustDecision, TrustLevel, TrustPrompt,
    TrustStore,
    gpu::{Camera, GpuSpeed, LayerRenderer},
    host::Frame,
    trust::{CodeSummary, TRUST_FILE},
};
use bri_package::packages::{PackageSet, Side};
use std::path::Path;
use std::sync::Arc;

/// The download cache under the content root, where a server's Add-Ons go.
const DOWNLOADS: &str = ".downloads/";

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

#[derive(Default)]
pub struct ClientCode {
    sandbox: Option<Sandbox>,
    code: Vec<AddOnCode>,
    /// Per `code`: the player's own `client` Add-On, not a server's.
    own: Vec<bool>,
    running: Vec<Running>,
    started: bool,
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
    /// Check the client code of every shared and client package in `set`.
    /// Problems are messages, not errors: the base game still runs.
    pub fn load(root: &Path, set: &PackageSet) -> Self {
        let mut out = Self::default();
        for entry in &set.packages {
            // Base game packages (listed with a role) never carry code.
            if entry.side == Side::Server || entry.role.is_some() {
                continue;
            }
            match AddOnCode::load(&root.join(&entry.dir)) {
                Ok(Some(code)) => {
                    out.code.push(code);
                    // A server only ever sends `shared` Add-Ons, and into
                    // the download cache.
                    out.own
                        .push(entry.side == Side::Client && !entry.dir.starts_with(DOWNLOADS));
                }
                Ok(None) => {}
                Err(problems) => {
                    for p in problems {
                        out.messages.push(format!(
                            "Add-On {} has code that cannot run: {} ({})",
                            entry.id, p.message, p.code
                        ));
                    }
                }
            }
        }
        out
    }

    /// Add-Ons with code, loaded and checked.
    pub fn code(&self) -> &[AddOnCode] {
        &self.code
    }

    pub fn is_started(&self) -> bool {
        self.started
    }

    /// Start the code the player trusts, when a game is entered.
    pub fn start(&mut self, host: Host<'_>, state_dir: &Path) {
        self.stop();
        self.started = true;
        self.time = 0.0;
        self.last = None;
        if self.code.is_empty() {
            return;
        }
        let trust = match host {
            Host::Local => None,
            Host::Remote(_) => Some(TrustStore::load(state_dir).unwrap_or_else(|e| {
                self.messages
                    .push(format!("Could not read {TRUST_FILE}: {e:#}"));
                TrustStore::default()
            })),
        };
        if self.sandbox.is_none() {
            match Sandbox::new() {
                Ok(sandbox) => self.sandbox = Some(sandbox),
                Err(e) => {
                    self.messages
                        .push(format!("Add-On code cannot run on this PC: {e:#}"));
                    return;
                }
            }
        }
        let sandbox = self.sandbox.as_ref().expect("created above");
        for (slot, code) in self.code.iter().enumerate() {
            let own = self.own[slot] && CodeSummary::from(code).tier() == Tier::Sandboxed;
            let granted = match (&host, &trust) {
                (Host::Local, _) => Some(TrustLevel::Sandboxed),
                _ if own => Some(TrustLevel::Sandboxed),
                (Host::Remote(""), _) => None,
                (Host::Remote(server), Some(store)) => {
                    store.granted(server, &CodeSummary::from(code))
                }
                (Host::Remote(_), None) => None,
            };
            let Some(granted) = granted else {
                let summary = CodeSummary::from(code);
                self.messages.push(if summary.tier() == Tier::Elevated {
                    format!(
                        "{}'s code is off: it asks for more than the sandbox allows",
                        code.name
                    )
                } else {
                    format!(
                        "{}'s code is off: you have not trusted this server to run it",
                        code.name
                    )
                });
                continue;
            };
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
                            Err(e) => self
                                .messages
                                .push(format!("{}: {name} does not play: {e}", code.name)),
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
                Err(reason) => self
                    .messages
                    .push(format!("{} stopped: {reason}", code.name)),
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
        // The player's own `client` Add-Ons run without asking.
        let code: Vec<CodeSummary> = self
            .code
            .iter()
            .zip(&self.own)
            .filter(|(_, own)| !**own)
            .map(|(code, _)| CodeSummary::from(code))
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
    /// cost too much, frame after frame, is stopped and its bodies go.
    pub fn advance_physics(
        &mut self,
        dt: f32,
        building: &crate::building::Building,
        pushers: &[crate::local_physics::Pusher],
        shots: &[crate::local_physics::Shot],
    ) -> anyhow::Result<()> {
        let messages = &mut self.messages;
        let mut result = Ok(());
        self.running.retain_mut(|r| {
            let started = std::time::Instant::now();
            let advanced = r.physics.advance(dt, building, pushers, shots);
            if let Err(e) = advanced {
                result = Err(e);
                return true;
            }
            if r.physics.is_empty() {
                return true;
            }
            let ms = started.elapsed().as_secs_f32() * 1000.0;
            match r.addon.report_physics_time(ms) {
                Ok(()) => true,
                Err(reason) => {
                    messages.push(format!("{} stopped: {reason}", r.addon.name));
                    false
                }
            }
        });
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
            .map_or(0.0, |last| (now - last).clamp(0.0, 0.25) as f32);
        self.last = Some(now);
        self.time += dt;
        let messages = &mut self.messages;
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
                        messages.push(format!("{name}: {line}"));
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
                    messages.push(format!("{} stopped: {reason}", r.addon.name));
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
        let messages = &mut self.messages;
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
                    messages.push(format!("{} stopped: {reason}", r.addon.name));
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
            self.messages.push(format!(
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

/// What the game shows this frame, for Add-On code that reads the world:
/// players and vehicles where they are drawn, the public Add-On state the
/// player receives, and the scene's lighting.
pub fn world_view(
    view: &crate::network::View,
    entities: &std::collections::BTreeMap<u64, bri_sim::session::EntityInfo>,
    players: &std::collections::BTreeMap<bri_world::OwnerId, bri_sim::player::PlayerState>,
    vehicles: &crate::vehicles::ClientVehicles,
    assets: &crate::vehicles::VehicleAssets,
    camera: &bri_render::scene::Camera,
    skeletons: std::collections::BTreeMap<u64, bri_client_sandbox::world::Skeleton>,
) -> bri_client_sandbox::World {
    use bri_client_sandbox::world::{AddOnState, Entity, Environment, Player, Vehicle, World};
    let players = players
        .iter()
        .map(|(owner, state)| Player {
            id: *owner,
            alive: view.vitals.get(owner).is_none_or(|v| v.alive),
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
        skeletons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_package::packages::PackageEntry;

    const HOST: &str = "host-key:00112233445566778899aabbccddeeff";

    /// The Spinning Cube as a server's Add-On (`shared`), whose code needs
    /// the player's trust on someone else's server.
    fn sample_set() -> (std::path::PathBuf, PackageSet) {
        sample_set_on(Side::Shared)
    }
    fn sample_set_on(side: Side) -> (std::path::PathBuf, PackageSet) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
        let set = PackageSet {
            schema_version: 1,
            packages: vec![PackageEntry {
                id: "spinning-cube".into(),
                version: "1.0.0".into(),
                side,
                dir: "samples/spinning-cube".into(),
                role: None,
            }],
        };
        (root, set)
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
        assert_eq!(code.take_messages(), ["Spinning Cube: spinning cube ready"]);
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
        let messages = code.take_messages();
        assert!(messages[0].contains("graphics card reset"), "{messages:?}");
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
