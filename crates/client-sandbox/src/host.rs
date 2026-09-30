//! The WebAssembly host. One [`Sandbox`] (a Wasmtime engine) per client;
//! one [`AddOn`] per running Add-On, each in its own store, so Add-Ons share
//! nothing: not memory, not handles, not messages.
//!
//! A module exports `memory`, and optionally `init()` (run once) and
//! `frame(time: f32, dt: f32)` (run every rendered frame). It imports host
//! functions from `bri`; see [`crate::capability::function_capability`] for
//! which capability each needs, and `docs/architecture/client-sandbox.md`
//! for their signatures.
//!
//! Budgets are enforced three ways: Wasmtime fuel bounds the instructions a
//! call may run (deterministic), an epoch deadline bounds its wall-clock
//! time (a backstop for anything fuel undercounts), and store limits bound
//! memory and tables. Host functions count what they are asked to do. Any
//! budget exceeded stops the Add-On for the session, with a reason the
//! player can read; the game carries on without it.
use crate::addon::AddOnCode;
use crate::bodies::{self, BodyState, PhysicsCommand, PlayerPose};
use crate::capability::{self, Capability, Tier};
use crate::shader::Shader;
use crate::trust::TrustLevel;
use crate::world::World;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use wasmtime::{
    Caller, Config, Engine, Extern, Instance, Linker, Module, OptLevel, Store, StoreLimits,
    StoreLimitsBuilder, Trap, TypedFunc,
};

/// Native stack Add-On code may use, beyond where the game called it.
/// Wasmtime runs Add-On code on the calling thread's stack, and the game
/// calls it from its main thread: 1 MiB on Windows, and already partly used
/// by the frame loop. With 512 KiB, endless recursion started from 480 KiB
/// deep aborted the whole game with a native stack overflow instead of
/// trapping (release build; 256 KiB deep in a debug build). 256 KiB still
/// allows thousands of nested calls; compilers for WebAssembly keep large
/// locals in linear memory, not here.
pub const MAX_WASM_STACK: usize = 256 * 1024;

/// How often the wall-clock deadline advances.
pub const EPOCH_TICK: Duration = Duration::from_millis(1);

/// Vertex layout shared with the shader prelude: position, normal, uv.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}
pub const VERTEX_BYTES: usize = std::mem::size_of::<Vertex>();

/// What one Add-On may use. Defaults are generous for presentation and far
/// below what would hurt the game.
#[derive(Debug, Clone)]
pub struct Budgets {
    /// Linear memory, in bytes.
    pub memory_bytes: usize,
    /// Instructions (Wasmtime fuel) for instantiation and `init`.
    pub init_fuel: u64,
    /// Instructions (Wasmtime fuel) for one `frame` call.
    pub frame_fuel: u64,
    /// Wall-clock time for instantiation and `init`.
    pub init_time: Duration,
    /// Wall-clock time for one `frame` call.
    pub frame_time: Duration,
    pub meshes: usize,
    pub mesh_vertices: usize,
    /// Total vertex and index bytes across every mesh.
    pub mesh_bytes: usize,
    pub materials: usize,
    pub draws_per_frame: usize,
    pub triangles_per_frame: u64,
    pub log_bytes_per_frame: usize,
    pub sounds_per_frame: usize,
    pub messages_per_frame: usize,
    pub message_bytes: usize,
    /// GPU time the Add-On's layer may take per frame, and for how many
    /// frames in a row it may exceed that before it is stopped. The
    /// renderer lowers the shader loop cap after every frame over budget,
    /// so strikes mean even the lowest cap is too slow.
    pub gpu_ms_per_frame: f32,
    pub gpu_strikes: u32,
    /// One frame over this much GPU time stops the Add-On at once.
    pub gpu_stop_ms: f32,
    /// `physics.local`: bodies and joints alive at once, and physics
    /// requests (create, join, push, remove) per frame.
    pub bodies: usize,
    pub joints: usize,
    pub physics_calls_per_frame: usize,
    /// Time the game may spend simulating the Add-On's bodies per frame,
    /// and for how many frames in a row it may exceed that.
    pub physics_ms_per_frame: f32,
    pub physics_strikes: u32,
    /// `avatar.pose`: players posed per frame.
    pub poses_per_frame: usize,
}
impl Default for Budgets {
    fn default() -> Self {
        Self {
            memory_bytes: 64 * 1024 * 1024,
            init_fuel: 500_000_000,
            frame_fuel: 20_000_000,
            init_time: Duration::from_secs(1),
            frame_time: Duration::from_millis(8),
            meshes: 1024,
            mesh_vertices: 65_536,
            mesh_bytes: 32 * 1024 * 1024,
            materials: 256,
            draws_per_frame: 2048,
            triangles_per_frame: 1_000_000,
            log_bytes_per_frame: 4096,
            sounds_per_frame: 16,
            messages_per_frame: 32,
            message_bytes: 16 * 1024,
            gpu_ms_per_frame: 4.0,
            gpu_strikes: 20,
            gpu_stop_ms: 100.0,
            bodies: 256,
            joints: 512,
            physics_calls_per_frame: 1024,
            physics_ms_per_frame: 4.0,
            physics_strikes: 30,
            poses_per_frame: 64,
        }
    }
}

/// Why an Add-On was stopped. Shown to the player; the game carries on.
#[derive(Debug, Clone, PartialEq)]
pub enum Stopped {
    /// Ran more instructions than one call allows.
    Cpu,
    /// Ran longer than one call allows.
    Time,
    /// Asked for more memory than its budget.
    Memory,
    /// Asked the host for more than its budget (meshes, draws, messages).
    Budget(String),
    /// Broke a host function's rules (bad pointer, bad handle, bad data).
    Misuse(String),
    /// The module itself trapped (a bug in the Add-On).
    Crashed(String),
    /// Its render layer took too much GPU time too many frames in a row,
    /// or the GPU refused it.
    Gpu(String),
    /// Its bodies took too much time to simulate too many frames in a row.
    Physics(String),
}
impl std::fmt::Display for Stopped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cpu => write!(f, "it used too much processing time"),
            Self::Time => write!(f, "it took too long to respond"),
            Self::Memory => write!(f, "it used too much memory"),
            Self::Budget(what) => write!(f, "it asked for too much: {what}"),
            Self::Misuse(what) => write!(f, "it broke a sandbox rule: {what}"),
            Self::Crashed(what) => write!(f, "it crashed: {what}"),
            Self::Gpu(what) => write!(f, "its graphics were too heavy: {what}"),
            Self::Physics(what) => write!(f, "its physics were too heavy: {what}"),
        }
    }
}
impl std::error::Error for Stopped {}

/// The client's WebAssembly engine, shared by every Add-On.
pub struct Sandbox {
    engine: Engine,
    ticker: Arc<Ticker>,
}

/// Advances the engine's epoch, the wall-clock deadline every call runs
/// under. Shared by the sandbox and every Add-On it started, so deadlines
/// keep advancing for as long as any Add-On can run.
struct Ticker {
    ticking: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Ticker {
    fn drop(&mut self) {
        self.ticking.store(false, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Sandbox {
    pub fn new() -> anyhow::Result<Self> {
        let mut config = Config::new();
        config
            .consume_fuel(true)
            .epoch_interruption(true)
            .max_wasm_stack(MAX_WASM_STACK)
            .cranelift_opt_level(OptLevel::Speed)
            .wasm_memory64(false)
            .wasm_multi_memory(false)
            .wasm_tail_call(false)
            .wasm_relaxed_simd(false)
            .wasm_extended_const(false)
            .wasm_custom_page_sizes(false)
            .wasm_wide_arithmetic(false);
        let engine = Engine::new(&config).map_err(|e| anyhow::anyhow!("{e}"))?;
        let ticking = Arc::new(AtomicBool::new(true));
        let thread = {
            let engine = engine.clone();
            let ticking = ticking.clone();
            std::thread::Builder::new()
                .name("addon-epoch".into())
                .spawn(move || {
                    while ticking.load(Ordering::Relaxed) {
                        std::thread::sleep(EPOCH_TICK);
                        engine.increment_epoch();
                    }
                })?
        };
        Ok(Self {
            engine,
            ticker: Arc::new(Ticker {
                ticking,
                thread: Some(thread),
            }),
        })
    }

    /// Compile and start an Add-On's client code. Its `start` function and
    /// `init` export run here, under the init budgets.
    ///
    /// `granted` is what the player chose on the trust prompt for exactly
    /// this code ([`crate::trust::TrustStore::granted`]); code that needs
    /// more never starts.
    pub fn start(
        &self,
        code: &AddOnCode,
        budgets: Budgets,
        granted: TrustLevel,
    ) -> Result<AddOn, Stopped> {
        if code.tier() == Tier::Elevated && granted != TrustLevel::Elevated {
            return Err(Stopped::Misuse(
                "it needs full trust, which you have not given this server".into(),
            ));
        }
        if let Some(c) = code.capabilities.iter().find(|c| !c.available()) {
            return Err(Stopped::Misuse(format!(
                "it needs `{}`, which this version of the game cannot run yet",
                c.name()
            )));
        }
        // Refused imports never reach the compiler.
        let problems = crate::addon::check_imports(&code.id, &code.module, &code.capabilities);
        if let Some(problem) = problems.first() {
            return Err(Stopped::Misuse(problem.message.clone()));
        }
        let module = Module::new(&self.engine, &code.module)
            .map_err(|e| Stopped::Misuse(format!("the module does not compile: {e}")))?;
        match module.get_export("memory") {
            Some(wasmtime::ExternType::Memory(_)) => {}
            _ => return Err(Stopped::Misuse("the module must export `memory`".into())),
        }
        let mut linker = Linker::new(&self.engine);
        link(&mut linker, &code.capabilities)
            .map_err(|e| Stopped::Misuse(format!("linking: {e}")))?;
        let state = HostState {
            limits: StoreLimitsBuilder::new()
                .memory_size(budgets.memory_bytes)
                .table_elements(100_000)
                .instances(1)
                .tables(4)
                .memories(1)
                .trap_on_grow_failure(true)
                .build(),
            shaders: code.shaders.clone(),
            sounds: code.sounds.iter().cloned().collect(),
            budgets: budgets.clone(),
            layer: Layer::default(),
            frame: Frame::default(),
            mesh_bytes: 0,
            inbox: VecDeque::new(),
            focused: false,
            keys: BTreeSet::new(),
            camera: [0.0, 0.0, 0.0, 0.0, 0.0, -1.0],
            world: Arc::new(World::default()),
            view: View::default(),
            kinds: Vec::new(),
            archetype_kinds: Vec::new(),
            image_kinds: Vec::new(),
            bodies: Arc::default(),
            live_bodies: BTreeSet::new(),
            joints: Vec::new(),
            next_body: 0,
            random: 0x9e37_79b9_7f4a_7c15
                ^ u64::from_str_radix(&code.code_hash[..16], 16).unwrap_or(1),
            violation: None,
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|s| &mut s.limits);
        store.set_fuel(budgets.init_fuel).expect("fuel is enabled");
        store.set_epoch_deadline(ticks(budgets.init_time));
        store.epoch_deadline_trap();
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| stopped(&mut store, e))?;
        let frame = instance
            .get_typed_func::<(f32, f32), ()>(&mut store, "frame")
            .ok();
        let mut addon = AddOn {
            id: code.id.clone(),
            name: code.name.clone(),
            store,
            instance,
            frame,
            stopped: None,
            gpu_strikes: 0,
            physics_strikes: 0,
            init_log: Vec::new(),
            _ticker: self.ticker.clone(),
        };
        if let Ok(init) = addon
            .instance
            .get_typed_func::<(), ()>(&mut addon.store, "init")
        {
            init.call(&mut addon.store, ())
                .map_err(|e| stopped(&mut addon.store, e))?;
        }
        // What start and init logged arrives with the first frame.
        addon.init_log = std::mem::take(&mut addon.store.data_mut().frame.log);
        Ok(addon)
    }
}

fn ticks(time: Duration) -> u64 {
    (time.as_micros() / EPOCH_TICK.as_micros()).max(1) as u64 + 1
}

/// Turn a failed call into the reason the Add-On stops.
fn stopped(store: &mut Store<HostState>, error: wasmtime::Error) -> Stopped {
    if let Some(v) = store.data_mut().violation.take() {
        return v;
    }
    if let Some(v) = error.downcast_ref::<Stopped>() {
        return v.clone();
    }
    match error.downcast_ref::<Trap>() {
        Some(Trap::OutOfFuel) => Stopped::Cpu,
        Some(Trap::Interrupt) => Stopped::Time,
        Some(Trap::StackOverflow) => Stopped::Crashed("stack overflow".into()),
        Some(trap) => Stopped::Crashed(trap.to_string()),
        None => {
            let text = format!("{error:#}");
            if text.contains("memory") && text.contains("limit") || text.contains("grow") {
                Stopped::Memory
            } else {
                Stopped::Crashed(text)
            }
        }
    }
}

/// One mesh the Add-On created. Meshes never change after creation, so
/// the renderer uploads each once.
#[derive(Debug, Clone)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct Material {
    /// Index into the Add-On's shaders.
    pub shader: usize,
    pub params: [[f32; 4]; 4],
    pub blend: Blend,
    pub space: Space,
}

/// What a material's draws are placed in, and what they draw over.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Space {
    /// The world, in world units: hidden behind walls like anything else.
    #[default]
    World,
    /// The camera's own space (x right, y up, looking down -z), seen at
    /// the player's normal field of view whatever the zoom, and drawn over
    /// the world so walls never cut into it: first-person arms and guns.
    View,
    /// The screen: x from -aspect (left) to aspect (right), y from -1
    /// (bottom) to 1 (top). Drawn over the world and view layers: a scope,
    /// a mask, a full-screen tint.
    Screen,
}
impl Space {
    pub const ALL: [Space; 3] = [Space::World, Space::View, Space::Screen];
    pub fn from_code(code: i32) -> Option<Self> {
        Self::ALL.get(usize::try_from(code).ok()?).copied()
    }
}

/// How a material's colour meets what is already drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Blend {
    /// Solid: writes depth; alpha blends over what is behind.
    #[default]
    Opaque,
    /// Glow: adds its colour (times alpha), writes no depth, both faces.
    Additive,
    /// See-through: alpha blends, writes no depth, both faces.
    Translucent,
}
impl Blend {
    pub const ALL: [Blend; 3] = [Blend::Opaque, Blend::Additive, Blend::Translucent];
    pub fn from_code(code: i32) -> Option<Self> {
        Self::ALL.get(usize::try_from(code).ok()?).copied()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Draw {
    pub mesh: usize,
    pub material: usize,
    /// Column-major model matrix, in world units.
    pub model: [f32; 16],
    /// This draw's own parameters in place of its material's (`draw_with`).
    pub params: Option<[[f32; 4]; 4]>,
}

/// Everything the Add-On's render layer holds.
#[derive(Debug, Default)]
pub struct Layer {
    pub meshes: Vec<Mesh>,
    pub materials: Vec<Material>,
}

/// What one `frame` call produced.
#[derive(Debug, Default, Clone)]
pub struct Frame {
    pub draws: Vec<Draw>,
    pub triangles: u64,
    /// Sounds to play (from the Add-On's own files).
    pub sounds: Vec<Sound>,
    /// Messages to the Add-On's server script.
    pub outbox: Vec<Vec<u8>>,
    pub log: Vec<String>,
    log_bytes: usize,
    /// Physics requests for the game, in the order made (`physics.local`).
    pub physics: Vec<PhysicsCommand>,
    /// Players' bodies to draw posed (`avatar.pose`), one each.
    pub poses: Vec<PlayerPose>,
}

/// One sound an Add-On asked for this frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Sound {
    /// One of the files its `client.sounds` lists.
    pub name: String,
    /// 0 to 1.
    pub volume: f32,
    /// Where in the world it plays (fading with distance), or `None` for
    /// the player's ears.
    pub at: Option<[f32; 3]>,
}

/// What the engine tells an Add-On each frame.
#[derive(Debug, Default, Clone)]
pub struct FrameInput {
    pub time: f32,
    pub dt: f32,
    /// Whether the Add-On's panel has keyboard focus. Keys are empty
    /// otherwise, whatever is pressed.
    pub focused: bool,
    pub keys_down: Vec<u32>,
    /// Messages from the Add-On's server script, oldest first.
    pub messages: Vec<Vec<u8>>,
    /// Where the player's camera is and looks, in world units (Y up).
    pub eye: [f32; 3],
    pub forward: [f32; 3],
    /// What the game shows this frame, for `world.read`.
    pub world: Arc<World>,
    /// The player's own view and held weapon, for `view`.
    pub view: View,
    /// This Add-On's bodies as the game last simulated them, for
    /// `rigid_get`.
    pub bodies: Arc<BTreeMap<u32, BodyState>>,
}

/// Floats `view` writes.
pub const VIEW_RECORD: usize = 12;

/// The player's own view this frame: what their screen is and shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// Horizontal field of view shown now (zoom and aim included), and the
    /// player's normal one, which [`Space::View`] draws at; degrees.
    pub fov: f32,
    pub normal_fov: f32,
    /// Screen size in pixels.
    pub size: [u32; 2],
    pub first_person: bool,
    /// Aiming down the held weapon's sights (`Image::zoom`).
    pub aiming: bool,
    pub alive: bool,
}
impl Default for View {
    fn default() -> Self {
        Self {
            fov: 90.0,
            normal_fov: 90.0,
            size: [1280, 720],
            first_person: true,
            aiming: false,
            alive: true,
        }
    }
}
impl View {
    pub fn aspect(&self) -> f32 {
        self.size[0].max(1) as f32 / self.size[1].max(1) as f32
    }
    /// The `view` record: fov, normal fov, aspect, width, height, flags
    /// (1 first person, 2 aiming, 4 alive), then padding.
    pub fn record(&self) -> [f32; VIEW_RECORD] {
        let flags = u8::from(self.first_person)
            | (u8::from(self.aiming) << 1)
            | (u8::from(self.alive) << 2);
        let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
        [
            finite(self.fov),
            finite(self.normal_fov),
            finite(self.aspect()),
            self.size[0] as f32,
            self.size[1] as f32,
            f32::from(flags),
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
        ]
    }
}

struct HostState {
    limits: StoreLimits,
    shaders: Vec<Shader>,
    sounds: BTreeSet<String>,
    budgets: Budgets,
    layer: Layer,
    frame: Frame,
    mesh_bytes: usize,
    inbox: VecDeque<Vec<u8>>,
    focused: bool,
    keys: BTreeSet<u32>,
    camera: [f32; 6],
    world: Arc<World>,
    view: View,
    /// Vehicle definitions the Add-On named with `vehicle_kind`.
    kinds: Vec<String>,
    /// Archetypes and weapon images it named with `archetype_kind` and
    /// `image_kind`, which `players` records report.
    archetype_kinds: Vec<String>,
    image_kinds: Vec<String>,
    /// The Add-On's bodies as last simulated.
    bodies: Arc<BTreeMap<u32, BodyState>>,
    /// Bodies it created and has not removed, and its joints' bodies.
    live_bodies: BTreeSet<u32>,
    joints: Vec<(u32, u32)>,
    next_body: u32,
    random: u64,
    /// Set by a host function just before it traps, so the reason survives.
    violation: Option<Stopped>,
}

/// A running Add-On.
pub struct AddOn {
    pub id: String,
    pub name: String,
    store: Store<HostState>,
    instance: Instance,
    frame: Option<TypedFunc<(f32, f32), ()>>,
    stopped: Option<Stopped>,
    gpu_strikes: u32,
    physics_strikes: u32,
    init_log: Vec<String>,
    _ticker: Arc<Ticker>,
}

impl AddOn {
    /// Run one frame. After the Add-On stops, every call returns why.
    pub fn frame(&mut self, input: FrameInput) -> Result<&Frame, Stopped> {
        if let Some(stopped) = &self.stopped {
            return Err(stopped.clone());
        }
        let budgets = self.store.data().budgets.clone();
        {
            let state = self.store.data_mut();
            state.frame = Frame {
                log: std::mem::take(&mut self.init_log),
                ..Frame::default()
            };
            state.focused = input.focused;
            let (eye, forward) = (input.eye, input.forward);
            state.camera = [eye[0], eye[1], eye[2], forward[0], forward[1], forward[2]];
            state.world = input.world;
            state.view = input.view;
            state.bodies = input.bodies;
            state.keys = if input.focused {
                input.keys_down.into_iter().collect()
            } else {
                BTreeSet::new()
            };
            for message in input.messages {
                if state.inbox.len() < 256 {
                    state.inbox.push_back(message);
                }
            }
        }
        if let Some(frame) = self.frame.clone() {
            self.store
                .set_fuel(budgets.frame_fuel)
                .expect("fuel is enabled");
            self.store.set_epoch_deadline(ticks(budgets.frame_time));
            if let Err(e) = frame.call(&mut self.store, (input.time, input.dt)) {
                let reason = stopped(&mut self.store, e);
                return Err(self.stop(reason));
            }
        }
        Ok(&self.store.data().frame)
    }

    /// Stop the Add-On and drop everything it holds.
    pub fn stop(&mut self, reason: Stopped) -> Stopped {
        let state = self.store.data_mut();
        state.layer = Layer::default();
        state.frame = Frame::default();
        state.inbox.clear();
        state.live_bodies.clear();
        state.joints.clear();
        self.stopped = Some(reason.clone());
        reason
    }

    pub fn stopped(&self) -> Option<&Stopped> {
        self.stopped.as_ref()
    }

    pub fn layer(&self) -> &Layer {
        &self.store.data().layer
    }

    pub fn shaders(&self) -> &[Shader] {
        &self.store.data().shaders
    }

    pub fn budgets(&self) -> &Budgets {
        &self.store.data().budgets
    }

    /// The game reports how long simulating the Add-On's bodies took this
    /// frame. Too many frames in a row over budget stops it; the game then
    /// drops its bodies.
    pub fn report_physics_time(&mut self, ms: f32) -> Result<(), Stopped> {
        if let Some(stopped) = &self.stopped {
            return Err(stopped.clone());
        }
        let budgets = &self.store.data().budgets;
        if ms > budgets.physics_ms_per_frame {
            self.physics_strikes += 1;
            if self.physics_strikes >= budgets.physics_strikes {
                let reason = Stopped::Physics(format!(
                    "{ms:.1} ms a frame for {} frames in a row (budget {:.1} ms)",
                    self.physics_strikes, budgets.physics_ms_per_frame
                ));
                return Err(self.stop(reason));
            }
        } else {
            self.physics_strikes = 0;
        }
        Ok(())
    }

    /// The renderer reports how long the Add-On's layer took on the GPU.
    /// One frame far over budget, or too many frames over it in a row,
    /// stops it.
    pub fn report_gpu_time(&mut self, ms: f32) -> Result<(), Stopped> {
        if let Some(stopped) = &self.stopped {
            return Err(stopped.clone());
        }
        let budgets = &self.store.data().budgets;
        if ms > budgets.gpu_stop_ms {
            let reason = Stopped::Gpu(format!(
                "one frame took {ms:.0} ms of graphics time (the limit is {:.0} ms)",
                budgets.gpu_stop_ms
            ));
            return Err(self.stop(reason));
        }
        if ms > budgets.gpu_ms_per_frame {
            self.gpu_strikes += 1;
            if self.gpu_strikes >= budgets.gpu_strikes {
                let reason = Stopped::Gpu(format!(
                    "{ms:.1} ms a frame for {} frames in a row (budget {:.1} ms)",
                    self.gpu_strikes, budgets.gpu_ms_per_frame
                ));
                return Err(self.stop(reason));
            }
        } else {
            self.gpu_strikes = 0;
        }
        Ok(())
    }
}

type Host<'a> = Caller<'a, HostState>;

fn violation(caller: &mut Host<'_>, reason: Stopped) -> wasmtime::Error {
    caller.data_mut().violation = Some(reason.clone());
    wasmtime::Error::new(reason)
}
fn misuse(caller: &mut Host<'_>, what: impl Into<String>) -> wasmtime::Error {
    violation(caller, Stopped::Misuse(what.into()))
}
fn over(caller: &mut Host<'_>, what: impl Into<String>) -> wasmtime::Error {
    violation(caller, Stopped::Budget(what.into()))
}

/// Copy `len` bytes at `ptr` out of the Add-On's memory.
fn read(caller: &mut Host<'_>, ptr: i32, len: i32, max: usize) -> wasmtime::Result<Vec<u8>> {
    let (ptr, len) = (ptr as u32 as usize, len as u32 as usize);
    if len > max {
        return Err(over(
            caller,
            format!("{len} bytes in one call (limit {max})"),
        ));
    }
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return Err(misuse(caller, "no exported memory"));
    };
    let data = memory.data(&caller);
    match ptr.checked_add(len).and_then(|end| data.get(ptr..end)) {
        Some(bytes) => Ok(bytes.to_vec()),
        None => Err(misuse(caller, "a pointer outside its memory")),
    }
}

fn write(caller: &mut Host<'_>, ptr: i32, bytes: &[u8]) -> wasmtime::Result<()> {
    let ptr = ptr as u32 as usize;
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return Err(misuse(caller, "no exported memory"));
    };
    let data = memory.data_mut(&mut *caller);
    match ptr
        .checked_add(bytes.len())
        .and_then(|end| data.get_mut(ptr..end))
    {
        Some(target) => {
            target.copy_from_slice(bytes);
            Ok(())
        }
        None => Err(misuse(caller, "a pointer outside its memory")),
    }
}

fn floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn text(caller: &mut Host<'_>, ptr: i32, len: i32, max: usize) -> wasmtime::Result<String> {
    let bytes = read(caller, ptr, len, max)?;
    String::from_utf8(bytes).map_err(|_| misuse(caller, "text that is not UTF-8"))
}

/// Queue one of the Add-On's sounds; past the frame's allowance it is
/// dropped (-1), since sounds are cosmetic.
fn queue_sound(
    caller: &mut Host<'_>,
    ptr: i32,
    len: i32,
    volume: f32,
    at: Option<[f32; 3]>,
) -> wasmtime::Result<i32> {
    let name = text(caller, ptr, len, 256)?;
    if !caller.data().sounds.contains(&name) {
        return Err(misuse(caller, format!("no sound `{name}` in this Add-On")));
    }
    let limit = caller.data().budgets.sounds_per_frame;
    let frame = &mut caller.data_mut().frame;
    if frame.sounds.len() >= limit {
        return Ok(-1);
    }
    frame.sounds.push(Sound {
        name,
        volume: if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            0.0
        },
        at,
    });
    Ok(0)
}

/// Queue one draw, with the material's parameters or its own (16 floats at
/// `params`).
fn push_draw(
    caller: &mut Host<'_>,
    mesh: i32,
    material: i32,
    matrix: i32,
    params: Option<i32>,
) -> wasmtime::Result<()> {
    let budgets = caller.data().budgets.clone();
    if caller.data().frame.draws.len() >= budgets.draws_per_frame {
        return Err(over(
            caller,
            format!("more than {} draws a frame", budgets.draws_per_frame),
        ));
    }
    let (mesh, material) = (mesh as u32 as usize, material as u32 as usize);
    let Some(triangles) = caller
        .data()
        .layer
        .meshes
        .get(mesh)
        .map(|m| m.indices.len() as u64 / 3)
    else {
        return Err(misuse(caller, format!("no mesh {mesh}")));
    };
    if material >= caller.data().layer.materials.len() {
        return Err(misuse(caller, format!("no material {material}")));
    }
    if caller.data().frame.triangles + triangles > budgets.triangles_per_frame {
        return Err(over(
            caller,
            format!(
                "more than {} triangles a frame",
                budgets.triangles_per_frame
            ),
        ));
    }
    let values = floats(&read(caller, matrix, 64, 64)?);
    if !values.iter().all(|v| v.is_finite() && v.abs() <= 1.0e6) {
        return Err(misuse(caller, "a transform that is not a finite number"));
    }
    let mut model = [0.0; 16];
    model.copy_from_slice(&values);
    let params = match params {
        Some(ptr) => {
            let values = floats(&read(caller, ptr, 64, 64)?);
            if !values.iter().all(|v| v.is_finite()) {
                return Err(misuse(caller, "a parameter that is not a finite number"));
            }
            let mut p = [[0.0; 4]; 4];
            for (i, v) in values.iter().enumerate() {
                p[i / 4][i % 4] = *v;
            }
            Some(p)
        }
        None => None,
    };
    let frame = &mut caller.data_mut().frame;
    frame.triangles += triangles;
    frame.draws.push(Draw {
        mesh,
        material,
        model,
        params,
    });
    Ok(())
}

/// The index of `name` (read from the Add-On's memory) in one of its kind
/// lists, adding it when new: records then report kinds as small numbers.
fn name_kind(
    caller: &mut Host<'_>,
    ptr: i32,
    len: i32,
    list: fn(&mut HostState) -> &mut Vec<String>,
    what: &str,
) -> wasmtime::Result<i32> {
    let name = text(caller, ptr, len, 160)?;
    let kinds = list(caller.data_mut());
    if let Some(i) = kinds.iter().position(|k| k.eq_ignore_ascii_case(&name)) {
        return Ok(i as i32);
    }
    if kinds.len() >= crate::world::MAX_KINDS {
        let n = crate::world::MAX_KINDS;
        return Err(over(caller, format!("more than {n} {what} kinds")));
    }
    kinds.push(name);
    Ok(kinds.len() as i32 - 1)
}

/// Define the host functions of every declared capability, and nothing
/// else: an import of anything undeclared cannot link.
fn link(linker: &mut Linker<HostState>, declared: &BTreeSet<Capability>) -> wasmtime::Result<()> {
    let m = capability::IMPORT_MODULE;
    let has = |c: Capability| declared.contains(&c);

    linker.func_wrap(m, "log", |mut caller: Host<'_>, ptr: i32, len: i32| {
        let line = text(&mut caller, ptr, len, 1024)?;
        let state = caller.data_mut();
        if state.frame.log_bytes + line.len() <= state.budgets.log_bytes_per_frame {
            state.frame.log_bytes += line.len();
            state.frame.log.push(line);
        }
        Ok(())
    })?;
    linker.func_wrap(m, "random", |mut caller: Host<'_>| -> i32 {
        // xorshift64*: presentation randomness only.
        let state = caller.data_mut();
        let mut x = state.random;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        state.random = x;
        (x.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 32) as i32
    })?;

    if has(Capability::RenderLayer) {
        linker.func_wrap(
            m,
            "mesh_create",
            |mut caller: Host<'_>,
             vptr: i32,
             vcount: i32,
             iptr: i32,
             icount: i32|
             -> wasmtime::Result<i32> {
                let budgets = caller.data().budgets.clone();
                let (vcount, icount) = (vcount as u32 as usize, icount as u32 as usize);
                if caller.data().layer.meshes.len() >= budgets.meshes {
                    return Err(over(
                        &mut caller,
                        format!("more than {} meshes", budgets.meshes),
                    ));
                }
                if vcount == 0 || vcount > budgets.mesh_vertices {
                    return Err(over(
                        &mut caller,
                        format!(
                            "a mesh of {vcount} vertices (limit {})",
                            budgets.mesh_vertices
                        ),
                    ));
                }
                if icount == 0 || icount % 3 != 0 {
                    return Err(misuse(
                        &mut caller,
                        "index count must be a positive multiple of 3",
                    ));
                }
                let bytes = vcount * VERTEX_BYTES + icount * 4;
                if caller.data().mesh_bytes + bytes > budgets.mesh_bytes {
                    return Err(over(
                        &mut caller,
                        format!("more than {} bytes of meshes", budgets.mesh_bytes),
                    ));
                }
                let v = read(
                    &mut caller,
                    vptr,
                    (vcount * VERTEX_BYTES) as i32,
                    budgets.mesh_bytes,
                )?;
                let i = read(&mut caller, iptr, (icount * 4) as i32, budgets.mesh_bytes)?;
                let values = floats(&v);
                if !values.iter().all(|x| x.is_finite() && x.abs() <= 1.0e6) {
                    return Err(misuse(&mut caller, "a vertex that is not a finite number"));
                }
                let vertices: Vec<Vertex> = bytemuck::pod_collect_to_vec(&v);
                let indices: Vec<u32> = i
                    .chunks_exact(4)
                    .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect();
                if indices.iter().any(|&x| x as usize >= vcount) {
                    return Err(misuse(&mut caller, "an index past the last vertex"));
                }
                let state = caller.data_mut();
                state.mesh_bytes += bytes;
                state.layer.meshes.push(Mesh { vertices, indices });
                Ok(state.layer.meshes.len() as i32 - 1)
            },
        )?;
        linker.func_wrap(
            m,
            "material_create",
            |mut caller: Host<'_>, shader: i32| -> wasmtime::Result<i32> {
                let state = caller.data();
                if state.layer.materials.len() >= state.budgets.materials {
                    let n = state.budgets.materials;
                    return Err(over(&mut caller, format!("more than {n} materials")));
                }
                if shader < 0 || shader as usize >= state.shaders.len() {
                    return Err(misuse(&mut caller, format!("no shader {shader}")));
                }
                let state = caller.data_mut();
                state.layer.materials.push(Material {
                    shader: shader as usize,
                    params: [[0.0; 4]; 4],
                    blend: Blend::Opaque,
                    space: Space::World,
                });
                Ok(state.layer.materials.len() as i32 - 1)
            },
        )?;
        linker.func_wrap(
            m,
            "camera",
            |mut caller: Host<'_>, ptr: i32| -> wasmtime::Result<()> {
                let bytes: Vec<u8> = caller
                    .data()
                    .camera
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect();
                write(&mut caller, ptr, &bytes)
            },
        )?;
        linker.func_wrap(
            m,
            "material_set",
            |mut caller: Host<'_>,
             material: i32,
             slot: i32,
             x: f32,
             y: f32,
             z: f32,
             w: f32|
             -> wasmtime::Result<()> {
                let values = [x, y, z, w];
                if !values.iter().all(|v| v.is_finite()) {
                    return Err(misuse(
                        &mut caller,
                        "a parameter that is not a finite number",
                    ));
                }
                let state = caller.data_mut();
                match state.layer.materials.get_mut(material as u32 as usize) {
                    Some(m) if (0..4).contains(&slot) => {
                        m.params[slot as usize] = values;
                        Ok(())
                    }
                    _ => Err(misuse(
                        &mut caller,
                        format!("no material {material} slot {slot}"),
                    )),
                }
            },
        )?;
        linker.func_wrap(
            m,
            "draw",
            |mut caller: Host<'_>, mesh: i32, material: i32, matrix: i32| -> wasmtime::Result<()> {
                push_draw(&mut caller, mesh, material, matrix, None)
            },
        )?;
        linker.func_wrap(
            m,
            "draw_with",
            |mut caller: Host<'_>,
             mesh: i32,
             material: i32,
             matrix: i32,
             params: i32|
             -> wasmtime::Result<()> {
                push_draw(&mut caller, mesh, material, matrix, Some(params))
            },
        )?;
        linker.func_wrap(
            m,
            "material_blend",
            |mut caller: Host<'_>, material: i32, mode: i32| -> wasmtime::Result<()> {
                let Some(blend) = Blend::from_code(mode) else {
                    return Err(misuse(&mut caller, format!("no blend mode {mode}")));
                };
                let state = caller.data_mut();
                match state.layer.materials.get_mut(material as u32 as usize) {
                    Some(m) => {
                        m.blend = blend;
                        Ok(())
                    }
                    None => Err(misuse(&mut caller, format!("no material {material}"))),
                }
            },
        )?;
        linker.func_wrap(
            m,
            "material_space",
            |mut caller: Host<'_>, material: i32, space: i32| -> wasmtime::Result<()> {
                let Some(space) = Space::from_code(space) else {
                    return Err(misuse(&mut caller, format!("no space {space}")));
                };
                let state = caller.data_mut();
                match state.layer.materials.get_mut(material as u32 as usize) {
                    Some(m) => {
                        m.space = space;
                        Ok(())
                    }
                    None => Err(misuse(&mut caller, format!("no material {material}"))),
                }
            },
        )?;
        linker.func_wrap(
            m,
            "view",
            |mut caller: Host<'_>, ptr: i32| -> wasmtime::Result<()> {
                let bytes: Vec<u8> = caller
                    .data()
                    .view
                    .record()
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect();
                write(&mut caller, ptr, &bytes)
            },
        )?;
        linker.func_wrap(
            m,
            "environment",
            |mut caller: Host<'_>, ptr: i32| -> wasmtime::Result<()> {
                let bytes: Vec<u8> = caller
                    .data()
                    .world
                    .environment_record()
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect();
                write(&mut caller, ptr, &bytes)
            },
        )?;
    }
    if has(Capability::RenderShader) {
        linker.func_wrap(
            m,
            "shader",
            |mut caller: Host<'_>, ptr: i32, len: i32| -> wasmtime::Result<i32> {
                let name = text(&mut caller, ptr, len, 256)?;
                match caller.data().shaders.iter().position(|s| s.name == name) {
                    Some(i) => Ok(i as i32),
                    None => Err(misuse(
                        &mut caller,
                        format!("no shader `{name}` in this Add-On"),
                    )),
                }
            },
        )?;
    }
    if has(Capability::Audio) {
        linker.func_wrap(
            m,
            "sound_play",
            |mut caller: Host<'_>, ptr: i32, len: i32, volume: f32| -> wasmtime::Result<i32> {
                queue_sound(&mut caller, ptr, len, volume, None)
            },
        )?;
        linker.func_wrap(
            m,
            "sound_at",
            |mut caller: Host<'_>,
             ptr: i32,
             len: i32,
             volume: f32,
             x: f32,
             y: f32,
             z: f32|
             -> wasmtime::Result<i32> {
                if ![x, y, z].iter().all(|v| v.is_finite() && v.abs() <= 1.0e6) {
                    return Err(misuse(&mut caller, "a place that is not a finite number"));
                }
                queue_sound(&mut caller, ptr, len, volume, Some([x, y, z]))
            },
        )?;
    }
    if has(Capability::InputFocused) {
        linker.func_wrap(m, "key_down", |caller: Host<'_>, key: i32| -> i32 {
            let state = caller.data();
            (state.focused && state.keys.contains(&(key as u32))) as i32
        })?;
    }
    if has(Capability::NetMessage) {
        linker.func_wrap(
            m,
            "send",
            |mut caller: Host<'_>, ptr: i32, len: i32| -> wasmtime::Result<i32> {
                let budgets = caller.data().budgets.clone();
                if caller.data().frame.outbox.len() >= budgets.messages_per_frame {
                    return Err(over(
                        &mut caller,
                        format!("more than {} messages a frame", budgets.messages_per_frame),
                    ));
                }
                let bytes = read(&mut caller, ptr, len, budgets.message_bytes)?;
                caller.data_mut().frame.outbox.push(bytes);
                Ok(0)
            },
        )?;
        linker.func_wrap(
            m,
            "recv",
            |mut caller: Host<'_>, ptr: i32, capacity: i32| -> wasmtime::Result<i32> {
                let Some(message) = caller.data().inbox.front().cloned() else {
                    return Ok(-1);
                };
                if message.len() > capacity as u32 as usize {
                    return Ok(-2 - message.len() as i32);
                }
                write(&mut caller, ptr, &message)?;
                caller.data_mut().inbox.pop_front();
                Ok(message.len() as i32)
            },
        )?;
    }
    if has(Capability::WorldRead) {
        linker.func_wrap(m, "local_player", |caller: Host<'_>| -> i32 {
            caller.data().world.local as i32
        })?;
        linker.func_wrap(
            m,
            "players",
            |mut caller: Host<'_>, ptr: i32, capacity: i32| -> wasmtime::Result<i32> {
                let state = caller.data();
                let records = state.world.player_records(
                    &state.archetype_kinds,
                    &state.image_kinds,
                    capacity.max(0) as usize,
                );
                let bytes: Vec<u8> = records.iter().flat_map(|v| v.to_le_bytes()).collect();
                write(&mut caller, ptr, &bytes)?;
                Ok((records.len() / crate::world::PLAYER_RECORD) as i32)
            },
        )?;
        linker.func_wrap(
            m,
            "entities",
            |mut caller: Host<'_>, ptr: i32, capacity: i32| -> wasmtime::Result<i32> {
                let records = caller.data().world.entity_records(capacity.max(0) as usize);
                let bytes: Vec<u8> = records.iter().flat_map(|v| v.to_le_bytes()).collect();
                write(&mut caller, ptr, &bytes)?;
                Ok((records.len() / crate::world::ENTITY_RECORD) as i32)
            },
        )?;
        linker.func_wrap(
            m,
            "vehicle_kind",
            |mut caller: Host<'_>, ptr: i32, len: i32| -> wasmtime::Result<i32> {
                name_kind(&mut caller, ptr, len, |s| &mut s.kinds, "vehicle")
            },
        )?;
        linker.func_wrap(
            m,
            "archetype_kind",
            |mut caller: Host<'_>, ptr: i32, len: i32| -> wasmtime::Result<i32> {
                name_kind(
                    &mut caller,
                    ptr,
                    len,
                    |s| &mut s.archetype_kinds,
                    "archetype",
                )
            },
        )?;
        linker.func_wrap(
            m,
            "image_kind",
            |mut caller: Host<'_>, ptr: i32, len: i32| -> wasmtime::Result<i32> {
                name_kind(&mut caller, ptr, len, |s| &mut s.image_kinds, "image")
            },
        )?;
        linker.func_wrap(
            m,
            "vehicles",
            |mut caller: Host<'_>, ptr: i32, capacity: i32| -> wasmtime::Result<i32> {
                let state = caller.data();
                let records = state
                    .world
                    .vehicle_records(&state.kinds, capacity.max(0) as usize);
                let bytes: Vec<u8> = records.iter().flat_map(|v| v.to_le_bytes()).collect();
                write(&mut caller, ptr, &bytes)?;
                Ok((records.len() / crate::world::VEHICLE_RECORD) as i32)
            },
        )?;
        linker.func_wrap(
            m,
            "state_num",
            |mut caller: Host<'_>,
             package_ptr: i32,
             package_len: i32,
             key_ptr: i32,
             key_len: i32,
             player: i32,
             index: i32|
             -> wasmtime::Result<f32> {
                let package = text(&mut caller, package_ptr, package_len, 64)?;
                let key = text(&mut caller, key_ptr, key_len, 64)?;
                Ok(caller
                    .data()
                    .world
                    .state_number(&package, &key, i64::from(player), index))
            },
        )?;
    }
    if has(Capability::PhysicsLocal) {
        link_physics(linker)?;
    }
    if has(Capability::AvatarPose) {
        link_pose(linker)?;
    }
    Ok(())
}

/// Count one physics request against the frame's allowance.
fn physics_call(caller: &mut Host<'_>, command: PhysicsCommand) -> wasmtime::Result<()> {
    let limit = caller.data().budgets.physics_calls_per_frame;
    if caller.data().frame.physics.len() >= limit {
        return Err(over(
            caller,
            format!("more than {limit} physics requests a frame"),
        ));
    }
    caller.data_mut().frame.physics.push(command);
    Ok(())
}

/// A handle the Add-On passed: one of its live bodies, or it misused one.
fn live_body(caller: &mut Host<'_>, body: i32) -> wasmtime::Result<u32> {
    let id = body as u32;
    if caller.data().live_bodies.contains(&id) {
        Ok(id)
    } else {
        Err(misuse(caller, format!("no body {body}")))
    }
}

/// `physics.local`: bodies and joints the game simulates on this PC only.
fn link_physics(linker: &mut Linker<HostState>) -> wasmtime::Result<()> {
    let m = capability::IMPORT_MODULE;
    linker.func_wrap(
        m,
        "rigid_create",
        |mut caller: Host<'_>, ptr: i32| -> wasmtime::Result<i32> {
            let values = floats(&read(
                &mut caller,
                ptr,
                (bodies::BODY_RECORD * 4) as i32,
                bodies::BODY_RECORD * 4,
            )?);
            let spec = bodies::body_spec(&values).map_err(|e| misuse(&mut caller, e))?;
            let limit = caller.data().budgets.bodies;
            if caller.data().live_bodies.len() >= limit {
                return Err(over(&mut caller, format!("more than {limit} bodies")));
            }
            let state = caller.data_mut();
            state.next_body += 1;
            let body = state.next_body;
            state.live_bodies.insert(body);
            physics_call(&mut caller, PhysicsCommand::Create { body, spec })?;
            Ok(body as i32)
        },
    )?;
    linker.func_wrap(
        m,
        "rigid_joint",
        |mut caller: Host<'_>, a: i32, b: i32, ptr: i32| -> wasmtime::Result<i32> {
            let (a, b) = (live_body(&mut caller, a)?, live_body(&mut caller, b)?);
            if a == b {
                return Err(misuse(&mut caller, "a joint from a body to itself"));
            }
            let values = floats(&read(
                &mut caller,
                ptr,
                (bodies::JOINT_RECORD * 4) as i32,
                bodies::JOINT_RECORD * 4,
            )?);
            let spec = bodies::joint_spec(&values).map_err(|e| misuse(&mut caller, e))?;
            let limit = caller.data().budgets.joints;
            if caller.data().joints.len() >= limit {
                return Err(over(&mut caller, format!("more than {limit} joints")));
            }
            caller.data_mut().joints.push((a, b));
            physics_call(&mut caller, PhysicsCommand::Joint { a, b, spec })?;
            Ok(caller.data().joints.len() as i32 - 1)
        },
    )?;
    linker.func_wrap(
        m,
        "rigid_remove",
        |mut caller: Host<'_>, body: i32| -> wasmtime::Result<()> {
            let body = live_body(&mut caller, body)?;
            let state = caller.data_mut();
            state.live_bodies.remove(&body);
            state.joints.retain(|(a, b)| *a != body && *b != body);
            physics_call(&mut caller, PhysicsCommand::Remove { body })
        },
    )?;
    linker.func_wrap(
        m,
        "rigid_push",
        |mut caller: Host<'_>, body: i32, x: f32, y: f32, z: f32| -> wasmtime::Result<()> {
            let body = live_body(&mut caller, body)?;
            if ![x, y, z].iter().all(|v| v.is_finite()) {
                return Err(misuse(&mut caller, "a push that is not a finite number"));
            }
            let velocity = glam::Vec3::new(x, y, z)
                .clamp_length_max(bodies::MAX_SPEED)
                .to_array();
            physics_call(&mut caller, PhysicsCommand::Push { body, velocity })
        },
    )?;
    linker.func_wrap(
        m,
        "rigid_get",
        |mut caller: Host<'_>, body: i32, ptr: i32| -> wasmtime::Result<i32> {
            let id = body as u32;
            let Some(state) = caller
                .data()
                .bodies
                .get(&id)
                .filter(|_| caller.data().live_bodies.contains(&id))
                .copied()
            else {
                return Ok(0);
            };
            let bytes: Vec<u8> = state
                .record()
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect();
            write(&mut caller, ptr, &bytes)?;
            Ok(1)
        },
    )?;
    Ok(())
}

/// `avatar.pose`: read players' bodies as drawn and pose them.
fn link_pose(linker: &mut Linker<HostState>) -> wasmtime::Result<()> {
    let m = capability::IMPORT_MODULE;
    linker.func_wrap(
        m,
        "skeleton",
        |mut caller: Host<'_>, player: i32, ptr: i32, capacity: i32| -> wasmtime::Result<i32> {
            let world = caller.data().world.clone();
            let Some(skeleton) = world.skeletons.get(&(player as u32 as u64)) else {
                return Ok(-1);
            };
            let records = skeleton.records(capacity.max(0) as usize);
            let bytes: Vec<u8> = records.iter().flat_map(|v| v.to_le_bytes()).collect();
            write(&mut caller, ptr, &bytes)?;
            Ok(skeleton.nodes.len().min(bodies::MAX_NODES) as i32)
        },
    )?;
    fn lookup(
        caller: &mut Host<'_>,
        player: i32,
        ptr: i32,
        len: i32,
        find: fn(&crate::world::Rig, &str) -> i32,
    ) -> wasmtime::Result<i32> {
        let name = text(caller, ptr, len, 160)?;
        Ok(caller
            .data()
            .world
            .skeletons
            .get(&(player as u32 as u64))
            .map_or(-1, |s| find(&s.rig, &name)))
    }
    linker.func_wrap(
        m,
        "skeleton_node",
        |mut caller: Host<'_>, player: i32, ptr: i32, len: i32| -> wasmtime::Result<i32> {
            lookup(&mut caller, player, ptr, len, crate::world::Rig::node)
        },
    )?;
    linker.func_wrap(
        m,
        "skeleton_part",
        |mut caller: Host<'_>, player: i32, ptr: i32, len: i32| -> wasmtime::Result<i32> {
            lookup(&mut caller, player, ptr, len, crate::world::Rig::part)
        },
    )?;
    linker.func_wrap(
        m,
        "pose",
        |mut caller: Host<'_>, player: i32, ptr: i32, count: i32| -> wasmtime::Result<i32> {
            let count = count as u32 as usize;
            if count > bodies::MAX_NODES {
                return Err(over(
                    &mut caller,
                    format!("more than {} nodes in one pose", bodies::MAX_NODES),
                ));
            }
            let bytes = count * bodies::POSE_RECORD * 4;
            let values = floats(&read(&mut caller, ptr, bytes as i32, bytes)?);
            let nodes = bodies::pose_nodes(&values).map_err(|e| misuse(&mut caller, e))?;
            let player = player as u32 as u64;
            let Some(drawn) = caller
                .data()
                .world
                .skeletons
                .get(&player)
                .map(|s| s.nodes.len())
            else {
                return Ok(0);
            };
            if let Some((node, ..)) = nodes.iter().find(|(n, ..)| *n as usize >= drawn) {
                return Err(misuse(&mut caller, format!("no node {node}")));
            }
            let limit = caller.data().budgets.poses_per_frame;
            let poses = &mut caller.data_mut().frame.poses;
            match poses.iter().position(|p| p.player == player) {
                Some(i) => poses[i].nodes.extend(nodes),
                None if poses.len() < limit => poses.push(PlayerPose { player, nodes }),
                None => {
                    return Err(over(
                        &mut caller,
                        format!("more than {limit} players posed a frame"),
                    ));
                }
            }
            Ok(1)
        },
    )?;
    Ok(())
}
