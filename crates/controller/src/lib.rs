//! Movement controllers from Add-On WebAssembly, on the host and on the
//! predicting client alike (`bri_motor::controller`). One module, the same
//! bytes on every side, run so that the same tick gives the same answer
//! everywhere:
//!
//! - **Fresh every step.** Each step instantiates the module anew, so it
//!   cannot remember anything the engine does not see: its memory between
//!   ticks is the 16 bytes in the player's state, which corrections restore
//!   and replays rerun. Purity is enforced, not asked for.
//! - **Deterministic arithmetic.** WebAssembly's float operations are
//!   IEEE-754 and correctly rounded; NaN bit patterns, the one thing left to
//!   the machine, are canonicalised. No threads, no relaxed SIMD, no clock,
//!   no randomness: the only import is the sweep, answered from the polygons
//!   the motor itself collides with.
//! - **Bounded by fuel.** A step has [`STEP_FUEL`] units of work; running
//!   out stops it, at the same instruction on every machine, and the motor
//!   runs that tick alone on both sides. No wall-clock deadline is used,
//!   since that would differ between host and client.
//!
//! # The interface (`api` 1)
//!
//! The module exports `memory`, `io() -> i32` (where its 128-byte record
//! lives) and `step() -> i32` (0: use the record; anything else: leave the
//! tick to the motor). It may import `bri.sweep(dx, dy, dz: f32, out: i32)
//! -> i32`: sweep the body by that vector, 1 when something is hit, writing
//! fraction and normal xyz (four f32) at `out`. Before `step` the host fills
//! the record; after it, reads the outputs back. Little-endian throughout:
//!
//! | Offset | Type | In/out | What |
//! |---|---|---|---|
//! | 0 | u32 | in | interface version, 1 |
//! | 4 | u32 | in | flags: 1 grounded, 2 crouched, 4 jump, 8 crouch, 16 jet, 32 submerged, 64 attack (the primary trigger) |
//! | 8 | f32 | in | tick length, seconds |
//! | 12 | f32 | in | body scale |
//! | 16 | f32×3 | in | feet |
//! | 28 | f32×3 | in/out | velocity |
//! | 40 | f32×3 | in | facing on the ground plane, unit |
//! | 52 | f32×3 | in | where the movement keys push, on the ground plane |
//! | 64 | f32 | in | pitch |
//! | 68 | u32 | out | flags: 1 the controller took the jump (the motor does not jump), 2 no motor gravity this tick, 4 the body faces the heading at 92 |
//! | 72 | u32 | in/out | animation intent (low 8 bits kept) |
//! | 76 | 16 bytes | in/out | the controller's memory |
//! | 92 | f32 | in/out | the body's heading, radians in the look's yaw convention: in, the one set last tick or else the look's; out, read when flag 4 is set (kept to 1/10000 rad); a tick without flag 4 turns the body back to the look |
//! | 96 | f32 | in | the archetype's gravity, units/s² |
//!
//! Bytes 100 to 127 are reserved and read as zero. A module that never sets
//! out flags 2 or 4 runs exactly as before they existed.
use anyhow::{Context, Result, ensure};
use bri_motor::controller::{
    Controller, ControllerSource, ControllerState, ControllerTick, MEMORY_BYTES, MoveController,
    Probe,
};
use glam::Vec3;
use std::sync::{Arc, OnceLock};
use wasmtime::{
    Caller, Config, Engine, Extern, InstancePre, Linker, Module, OptLevel, Store, StoreLimits,
    StoreLimitsBuilder,
};

/// Units of work one step may do. A wall kick uses a few hundred; this
/// leaves room for real controllers while bounding a step to well under a
/// millisecond.
pub const STEP_FUEL: u64 = 200_000;
/// Memory a controller instance may grow to (a compiled Rust module asks
/// for about 1.1 MiB of stack and data).
pub const MAX_MEMORY: usize = 4 * 1024 * 1024;
/// WebAssembly stack for one step.
const MAX_WASM_STACK: usize = 256 * 1024;
/// Size of the exchange record.
pub const RECORD: usize = 128;
/// Interface version the record carries.
pub const API: u32 = 1;

const GROUNDED: u32 = 1;
const CROUCHED: u32 = 2;
const JUMP: u32 = 4;
const CROUCH: u32 = 8;
const JET: u32 = 16;
const SUBMERGED: u32 = 32;
const ATTACK: u32 = 64;
const TOOK_JUMP: u32 = 1;
const NO_GRAVITY: u32 = 2;
const FACES: u32 = 4;

fn engine() -> Result<&'static Engine> {
    static ENGINE: OnceLock<std::result::Result<Engine, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            // Threads are not compiled in (no `threads` feature), so a
            // module cannot share memory or race.
            let mut config = Config::new();
            config
                .consume_fuel(true)
                .cranelift_nan_canonicalization(true)
                .cranelift_opt_level(OptLevel::Speed)
                .max_wasm_stack(MAX_WASM_STACK)
                .wasm_relaxed_simd(false)
                .wasm_memory64(false)
                .wasm_multi_memory(false);
            Engine::new(&config).map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| anyhow::anyhow!("Controller runtime: {e}"))
}

struct Host {
    limits: StoreLimits,
    probe: Probe,
    /// A misuse found inside an import, reported as the step's error.
    fault: Option<String>,
}

/// A compiled controller module.
pub struct WasmController {
    id: String,
    pre: InstancePre<Host>,
}
impl std::fmt::Debug for WasmController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WasmController({})", self.id)
    }
}

/// Compile an archetype's controller: the module is checked against the
/// interface here, so a broken one is refused when the Add-On loads, not
/// mid-game.
pub fn compile(source: &ControllerSource) -> Result<Controller> {
    source.validate()?;
    let engine = engine()?;
    let module = Module::new(engine, &source.module)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .with_context(|| format!("Controller {} does not compile", source.id))?;
    for import in module.imports() {
        ensure!(
            import.module() == "bri" && matches!(import.name(), "sweep" | "sweep_from"),
            "Controller {} imports {}.{}; a controller may import only bri.sweep and bri.sweep_from",
            source.id,
            import.module(),
            import.name()
        );
    }
    for export in ["memory", "io", "step"] {
        ensure!(
            module.get_export(export).is_some(),
            "Controller {} must export `{export}`",
            source.id
        );
    }
    let mut linker = Linker::<Host>::new(engine);
    linker
        .func_wrap(
            "bri",
            "sweep",
            |caller: Caller<'_, Host>, dx: f32, dy: f32, dz: f32, out: i32| -> i32 {
                sweep(caller, Vec3::ZERO, Vec3::new(dx, dy, dz), out)
            },
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    linker
        .func_wrap(
            "bri",
            "sweep_from",
            |caller: Caller<'_, Host>,
             ox: f32,
             oy: f32,
             oz: f32,
             dx: f32,
             dy: f32,
             dz: f32,
             out: i32|
             -> i32 {
                sweep(caller, Vec3::new(ox, oy, oz), Vec3::new(dx, dy, dz), out)
            },
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let pre = linker
        .instantiate_pre(&module)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .with_context(|| format!("Controller {}", source.id))?;
    Ok(Controller(Arc::new(WasmController {
        id: source.id.clone(),
        pre,
    })))
}

/// `bri.sweep` and `bri.sweep_from`: sweep the body, write fraction and
/// normal at `out`, 1 when something is hit.
fn sweep(mut caller: Caller<'_, Host>, offset: Vec3, delta: Vec3, out: i32) -> i32 {
    let hit = match caller.data_mut().probe.sweep_from(offset, delta) {
        Ok(hit) => hit,
        Err(why) => {
            caller.data_mut().fault.get_or_insert(why);
            None
        }
    };
    let words = match hit {
        Some(h) => [h.fraction, h.normal.x, h.normal.y, h.normal.z],
        None => [1.0, 0.0, 0.0, 0.0],
    };
    let mut bytes = [0u8; 16];
    for (i, w) in words.iter().enumerate() {
        bytes[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return 0;
    };
    if u32::try_from(out)
        .ok()
        .and_then(|at| memory.write(&mut caller, at as usize, &bytes).ok())
        .is_none()
    {
        caller
            .data_mut()
            .fault
            .get_or_insert_with(|| "a sweep result outside its memory".into());
        return 0;
    }
    i32::from(hit.is_some())
}

fn put(record: &mut [u8; RECORD], at: usize, v: u32) {
    record[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn put_f(record: &mut [u8; RECORD], at: usize, v: f32) {
    put(record, at, v.to_bits());
}
fn put_v(record: &mut [u8; RECORD], at: usize, v: Vec3) {
    for (i, c) in v.to_array().into_iter().enumerate() {
        put_f(record, at + i * 4, c);
    }
}
fn get(record: &[u8; RECORD], at: usize) -> u32 {
    u32::from_le_bytes(record[at..at + 4].try_into().expect("four bytes"))
}
fn get_f(record: &[u8; RECORD], at: usize) -> f32 {
    f32::from_bits(get(record, at))
}

impl WasmController {
    fn run(&self, tick: &mut ControllerTick, probe: &mut Probe) -> std::result::Result<(), String> {
        let engine = self.pre.module().engine();
        let mut store = Store::new(
            engine,
            Host {
                limits: StoreLimitsBuilder::new()
                    .memory_size(MAX_MEMORY)
                    .instances(1)
                    .memories(1)
                    .tables(1)
                    .table_elements(10_000)
                    .trap_on_grow_failure(true)
                    .build(),
                probe: std::mem::take(probe),
                fault: None,
            },
        );
        store.limiter(|h| &mut h.limits);
        store.set_fuel(STEP_FUEL).map_err(|e| e.to_string())?;
        let result = self.run_in(&mut store, tick);
        let host = store.into_data();
        *probe = host.probe;
        match (result, host.fault) {
            (_, Some(fault)) => Err(fault),
            (result, None) => result,
        }
    }
    fn run_in(
        &self,
        store: &mut Store<Host>,
        tick: &mut ControllerTick,
    ) -> std::result::Result<(), String> {
        let stopped = |e: wasmtime::Error| {
            if store_out_of_fuel(&e) {
                format!("ran out of its {STEP_FUEL} units of work")
            } else {
                format!("stopped: {e}")
            }
        };
        let instance = self.pre.instantiate(&mut *store).map_err(stopped)?;
        let memory = instance
            .get_memory(&mut *store, "memory")
            .ok_or("`memory` is not a memory")?;
        let io = instance
            .get_typed_func::<(), i32>(&mut *store, "io")
            .map_err(|_| "`io` is not a function () -> i32")?;
        let step = instance
            .get_typed_func::<(), i32>(&mut *store, "step")
            .map_err(|_| "`step` is not a function () -> i32")?;
        let at = io.call(&mut *store, ()).map_err(stopped)?;
        let at = usize::try_from(at)
            .ok()
            .filter(|at| {
                at.checked_add(RECORD)
                    .is_some_and(|end| end <= memory.data_size(&*store))
            })
            .ok_or("its record lies outside its memory")?;
        let mut record = [0u8; RECORD];
        put(&mut record, 0, API);
        let flag = |on: bool, bit: u32| if on { bit } else { 0 };
        put(
            &mut record,
            4,
            flag(tick.grounded, GROUNDED)
                | flag(tick.crouched, CROUCHED)
                | flag(tick.jump, JUMP)
                | flag(tick.crouch, CROUCH)
                | flag(tick.jet, JET)
                | flag(tick.submerged, SUBMERGED)
                | flag(tick.attack, ATTACK),
        );
        put_f(&mut record, 8, tick.dt);
        put_f(&mut record, 12, tick.scale);
        put_v(&mut record, 16, tick.feet);
        put_v(&mut record, 28, tick.velocity);
        put_v(&mut record, 40, tick.facing);
        put_v(&mut record, 52, tick.wish);
        put_f(&mut record, 64, tick.pitch);
        put(&mut record, 72, u32::from(tick.state.intent));
        record[76..76 + MEMORY_BYTES].copy_from_slice(&tick.state.memory);
        // The body's heading: the controller's own if it set one, else the
        // look's.
        let look = tick.facing.x.atan2(-tick.facing.z);
        put_f(&mut record, 92, tick.state.heading().unwrap_or(look));
        put_f(&mut record, 96, tick.gravity);
        memory
            .write(&mut *store, at, &record)
            .map_err(|e| e.to_string())?;
        if step.call(&mut *store, ()).map_err(stopped)? != 0 {
            return Ok(());
        }
        memory
            .read(&*store, at, &mut record)
            .map_err(|e| e.to_string())?;
        tick.velocity = Vec3::new(get_f(&record, 28), get_f(&record, 32), get_f(&record, 36));
        if !tick.velocity.is_finite() {
            return Err("a velocity that is not a number".into());
        }
        let out = get(&record, 68);
        tick.take_jump = out & TOOK_JUMP != 0;
        tick.skip_gravity = out & NO_GRAVITY != 0;
        let mut memory_out = [0u8; MEMORY_BYTES];
        memory_out.copy_from_slice(&record[76..76 + MEMORY_BYTES]);
        let mut state = ControllerState {
            memory: memory_out,
            intent: get(&record, 72) as u8,
            heading: None,
        };
        if out & FACES != 0 {
            let heading = get_f(&record, 92);
            if !heading.is_finite() {
                return Err("a heading that is not a number".into());
            }
            state.set_heading(Some(heading));
        }
        tick.state = state;
        Ok(())
    }
}
fn store_out_of_fuel(e: &wasmtime::Error) -> bool {
    e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)
}
impl MoveController for WasmController {
    fn id(&self) -> &str {
        &self.id
    }
    fn step(
        &self,
        tick: &mut ControllerTick,
        probe: &mut Probe,
    ) -> std::result::Result<(), String> {
        // A failed step changes nothing: the tick goes to the motor alone.
        let mut trial = tick.clone();
        self.run(&mut trial, probe)?;
        *tick = trial;
        Ok(())
    }
}

/// Bind every controller in an archetype table (`compile` for each
/// distinct module); returns the archetypes whose module was refused.
pub fn bind(archetypes: &mut bri_motor::archetype::Archetypes) -> Vec<(String, anyhow::Error)> {
    archetypes.bind_controllers(&mut compile)
}
