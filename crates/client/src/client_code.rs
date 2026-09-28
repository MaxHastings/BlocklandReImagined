//! Add-On client code in a game session: the sandboxed WebAssembly and
//! shaders enabled Add-Ons carry (`docs/architecture/client-sandbox.md`).
//!
//! Code runs only while a game is entered, and only what the player
//! trusts: in a game this player hosts, their own enabled Add-Ons; on
//! someone else's server, what `addon-trust.json` grants for exactly that
//! code. Everything else is listed and skipped. An Add-On that breaks a
//! budget is stopped with one message; the game carries on.
use bri_client_sandbox::{
    AddOn, AddOnCode, Budgets, FrameInput, Sandbox, TrustLevel, TrustStore,
    gpu::{Camera, GpuSpeed, LayerRenderer},
    host::Frame,
    trust::{CodeSummary, TRUST_FILE},
};
use bri_package::packages::{PackageSet, Side};
use std::path::Path;

struct Running {
    addon: AddOn,
    renderer: Option<LayerRenderer>,
    frame: Frame,
}

/// Who runs the game being entered, for the trust decision.
pub enum Host<'a> {
    /// This player hosts it; their enabled Add-Ons are their own choice.
    Local,
    /// Someone else's server, by its identity key.
    Remote(&'a str),
}

#[derive(Default)]
pub struct ClientCode {
    sandbox: Option<Sandbox>,
    code: Vec<AddOnCode>,
    running: Vec<Running>,
    started: bool,
    time: f32,
    last: Option<f64>,
    messages: Vec<String>,
    /// This device's measured speed, once calibrated (`Some(None)` when
    /// calibration failed and the low default cap stays).
    speed: Option<Option<GpuSpeed>>,
}

impl ClientCode {
    /// Check the client code of every shared and client package in `set`.
    /// Problems are messages, not errors: the base game still runs.
    pub fn load(root: &Path, set: &PackageSet) -> Self {
        let mut out = Self::default();
        for entry in &set.packages {
            if entry.side == Side::Server {
                continue;
            }
            match AddOnCode::load(&root.join(&entry.dir)) {
                Ok(Some(code)) => out.code.push(code),
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
        for code in &self.code {
            let granted = match (&host, &trust) {
                (Host::Local, _) => Some(TrustLevel::Sandboxed),
                (Host::Remote(server), Some(store)) => {
                    store.granted(server, &CodeSummary::from(code))
                }
                (Host::Remote(_), None) => None,
            };
            let Some(granted) = granted else {
                self.messages.push(format!(
                    "{}'s code is off: you have not trusted this server to run it",
                    code.name
                ));
                continue;
            };
            match sandbox.start(code, Budgets::default(), granted) {
                Ok(addon) => self.running.push(Running {
                    addon,
                    renderer: None,
                    frame: Frame::default(),
                }),
                Err(reason) => self
                    .messages
                    .push(format!("{} stopped: {reason}", code.name)),
            }
        }
    }

    /// Stop everything, when the game is left.
    pub fn stop(&mut self) {
        self.running.clear();
        self.started = false;
    }

    /// Run every Add-On's `frame` for the frame rendered at `now` (seconds
    /// on any steady clock).
    pub fn run_frame(&mut self, now: f64, eye: glam::Vec3, forward: glam::Vec3) {
        let dt = self
            .last
            .map_or(0.0, |last| (now - last).clamp(0.0, 0.25) as f32);
        self.last = Some(now);
        self.time += dt;
        let messages = &mut self.messages;
        self.running.retain_mut(|r| {
            let input = FrameInput {
                time: self.time,
                dt,
                eye: eye.to_array(),
                forward: forward.to_array(),
                ..Default::default()
            };
            let name = r.addon.name.clone();
            match r.addon.frame(input) {
                Ok(frame) => {
                    for line in &frame.log {
                        messages.push(format!("{name}: {line}"));
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
        pixels: u64,
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
                pixels,
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

#[cfg(test)]
mod tests {
    use super::*;
    use bri_package::packages::PackageEntry;

    fn sample_set() -> (std::path::PathBuf, PackageSet) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
        let set = PackageSet {
            schema_version: 1,
            packages: vec![PackageEntry {
                id: "spinning-cube".into(),
                version: "1.0.0".into(),
                side: Side::Client,
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
        code.run_frame(0.0, glam::Vec3::new(10.0, 2.0, 5.0), glam::Vec3::X);
        let placed = code.running[0].frame.draws[0].model;
        // Three units ahead of where the camera was.
        assert_eq!([placed[12], placed[14]], [13.0, 5.0]);
        assert_eq!(code.take_messages(), ["Spinning Cube: spinning cube ready"]);
        code.stop();
        assert!(code.running().is_empty());
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
        code.run_frame(0.0, glam::Vec3::ZERO, glam::Vec3::X);
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
        code.start(Host::Remote("203.0.113.10:28000"), state.path());
        code.run_frame(0.0, glam::Vec3::ZERO, glam::Vec3::X);
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
        code.start(Host::Remote("203.0.113.10:28000"), state.path());
        assert!(code.running().is_empty());
        assert!(code.take_messages()[0].contains("not trusted this server"));

        let mut store = TrustStore::default();
        let summary = CodeSummary::from(&code.code()[0]);
        let bri_client_sandbox::TrustDecision::Ask(prompt) =
            store.decide("203.0.113.10:28000", "Test", &[summary])
        else {
            panic!()
        };
        store.accept(&prompt, "Test");
        store.save(state.path()).unwrap();
        code.start(Host::Remote("203.0.113.10:28000"), state.path());
        assert_eq!(code.running(), ["Spinning Cube"]);
    }
}
