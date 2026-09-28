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
use crate::capability::{self, Capability, Tier};
use crate::shader::Shader;
use crate::trust::TrustLevel;
use std::collections::{BTreeSet, VecDeque};
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
}

#[derive(Debug, Clone, Copy)]
pub struct Draw {
    pub mesh: usize,
    pub material: usize,
    /// Column-major model matrix, in world units.
    pub model: [f32; 16],
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
    /// Sound files (from the Add-On) and volume.
    pub sounds: Vec<(String, f32)>,
    /// Messages to the Add-On's server script.
    pub outbox: Vec<Vec<u8>>,
    pub log: Vec<String>,
    log_bytes: usize,
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
                let budgets = caller.data().budgets.clone();
                if caller.data().frame.draws.len() >= budgets.draws_per_frame {
                    return Err(over(
                        &mut caller,
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
                    return Err(misuse(&mut caller, format!("no mesh {mesh}")));
                };
                if material >= caller.data().layer.materials.len() {
                    return Err(misuse(&mut caller, format!("no material {material}")));
                }
                if caller.data().frame.triangles + triangles > budgets.triangles_per_frame {
                    return Err(over(
                        &mut caller,
                        format!(
                            "more than {} triangles a frame",
                            budgets.triangles_per_frame
                        ),
                    ));
                }
                let values = floats(&read(&mut caller, matrix, 64, 64)?);
                if !values.iter().all(|v| v.is_finite() && v.abs() <= 1.0e6) {
                    return Err(misuse(
                        &mut caller,
                        "a transform that is not a finite number",
                    ));
                }
                let mut model = [0.0; 16];
                model.copy_from_slice(&values);
                let frame = &mut caller.data_mut().frame;
                frame.triangles += triangles;
                frame.draws.push(Draw {
                    mesh,
                    material,
                    model,
                });
                Ok(())
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
                let name = text(&mut caller, ptr, len, 256)?;
                if !caller.data().sounds.contains(&name) {
                    return Err(misuse(
                        &mut caller,
                        format!("no sound `{name}` in this Add-On"),
                    ));
                }
                let limit = caller.data().budgets.sounds_per_frame;
                let frame = &mut caller.data_mut().frame;
                if frame.sounds.len() >= limit {
                    return Ok(-1); // Dropped, not fatal: sounds are cosmetic.
                }
                frame.sounds.push((
                    name,
                    if volume.is_finite() {
                        volume.clamp(0.0, 1.0)
                    } else {
                        0.0
                    },
                ));
                Ok(0)
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
    Ok(())
}
