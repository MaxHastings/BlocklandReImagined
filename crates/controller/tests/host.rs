//! The controller host's rules, each against a small module: a step cannot
//! remember anything the engine does not see, runs out of work at the same
//! point everywhere, cannot import anything but the sweep, and a failed
//! step changes nothing.
use bri_controller::compile;
use bri_motor::controller::{ControllerSource, ControllerState, ControllerTick, MAX_SWEEPS, Probe};
use glam::Vec3;
use std::path::PathBuf;

fn source(wat: &str) -> ControllerSource {
    ControllerSource {
        id: "test:controller/t".into(),
        module: wat::parse_str(wat).unwrap(),
    }
}
fn tick() -> ControllerTick {
    ControllerTick {
        dt: 0.032,
        scale: 1.0,
        feet: Vec3::ZERO,
        facing: Vec3::NEG_Z,
        wish: Vec3::ZERO,
        pitch: 0.0,
        grounded: false,
        crouched: false,
        submerged: false,
        jump: false,
        crouch: false,
        jet: false,
        velocity: Vec3::new(1.0, 2.0, 3.0),
        take_jump: false,
        state: ControllerState::default(),
    }
}
fn probe() -> Probe {
    Probe::new(Default::default(), Vec3::ZERO, 0.625, 2.65, 8.0)
}
fn step(wat: &str, tick: &mut ControllerTick) -> Result<(), String> {
    let controller = compile(&source(wat)).unwrap();
    controller.0.step(tick, &mut probe())
}

#[test]
fn the_samples_are_built_from_their_source() {
    let samples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/samples");
    for (package, name) in [("sample-wall-kick", "wall_kick"), ("sample-dive", "dive")] {
        let dir = samples.join(package).join("controller");
        let built = wat::parse_file(dir.join(format!("{name}.wat"))).unwrap();
        let path = dir.join(format!("{name}.wasm"));
        if std::env::var_os("BRI_BLESS").is_some() {
            std::fs::write(&path, &built).unwrap();
        }
        assert_eq!(
            std::fs::read(&path).unwrap(),
            built,
            "{name}.wasm is stale; rerun with BRI_BLESS=1"
        );
        compile(&ControllerSource {
            id: format!("{package}:controller/{name}"),
            module: built,
        })
        .unwrap();
    }
}

/// Velocity x counts the steps a module has seen, in a global and in its
/// memory: always 1, because every step starts from a fresh instance.
#[test]
fn a_step_cannot_remember_what_the_engine_does_not_see() {
    let wat = r#"(module
      (memory (export "memory") 1)
      (global $calls (mut i32) (i32.const 0))
      (func (export "io") (result i32) (i32.const 256))
      (func (export "step") (result i32)
        (global.set $calls (i32.add (global.get $calls) (i32.const 1)))
        (i32.store (i32.const 0) (i32.add (i32.load (i32.const 0)) (i32.const 1)))
        (f32.store (i32.const 284)
          (f32.convert_i32_u (i32.add (global.get $calls) (i32.load (i32.const 0)))))
        ;; What it keeps in the record's memory does carry over.
        (i32.store8 (i32.const 332) (i32.add (i32.load8_u (i32.const 332)) (i32.const 1)))
        (i32.const 0)))"#;
    let controller = compile(&source(wat)).unwrap();
    let mut t = tick();
    for round in 1..=3_u8 {
        controller.0.step(&mut t, &mut probe()).unwrap();
        assert_eq!(t.velocity.x, 2.0, "fresh globals and memory every step");
        assert_eq!(t.state.memory[0], round, "the record's memory persists");
    }
}

#[test]
fn a_step_that_runs_out_of_work_changes_nothing() {
    let wat = r#"(module
      (memory (export "memory") 1)
      (func (export "io") (result i32) (i32.const 0))
      (func (export "step") (result i32)
        (f32.store (i32.const 28) (f32.const 99))
        (loop $forever (br $forever))
        (i32.const 0)))"#;
    let mut t = tick();
    let error = step(wat, &mut t).unwrap_err();
    assert!(error.contains("units of work"), "{error}");
    assert_eq!(t, tick());
}

#[test]
fn a_controller_imports_only_the_sweep() {
    let wat = r#"(module
      (import "wasi_snapshot_preview1" "clock_time_get"
        (func (param i32 i64 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "io") (result i32) (i32.const 0))
      (func (export "step") (result i32) (i32.const 0)))"#;
    let error = compile(&source(wat)).unwrap_err();
    assert!(format!("{error:#}").contains("only bri.sweep"), "{error:#}");
    let missing = r#"(module (memory (export "memory") 1))"#;
    assert!(compile(&source(missing)).is_err());
}

#[test]
fn a_velocity_that_is_not_a_number_is_refused() {
    let wat = r#"(module
      (memory (export "memory") 1)
      (func (export "io") (result i32) (i32.const 0))
      (func (export "step") (result i32)
        (f32.store (i32.const 28) (f32.div (f32.const 0) (f32.const 0)))
        (i32.const 0)))"#;
    let mut t = tick();
    assert!(step(wat, &mut t).is_err());
    assert_eq!(t, tick());
}

/// NaN's bits are the one float result WebAssembly leaves to the machine;
/// the host canonicalises them, so what a controller stores is the same
/// everywhere.
#[test]
fn nan_bits_are_the_same_on_every_machine() {
    let wat = r#"(module
      (memory (export "memory") 1)
      (func (export "io") (result i32) (i32.const 0))
      (func (export "step") (result i32)
        (i32.store (i32.const 76)
          (i32.reinterpret_f32
            (f32.mul (f32.div (f32.const 0) (f32.const 0)) (f32.const -1))))
        (i32.const 0)))"#;
    let mut t = tick();
    step(wat, &mut t).unwrap();
    assert_eq!(
        u32::from_le_bytes(t.state.memory[..4].try_into().unwrap()),
        0x7fc0_0000
    );
}

#[test]
fn sweeps_are_bounded() {
    let sweeps = |n: u32, length: f32| {
        format!(
            r#"(module
          (import "bri" "sweep" (func $sweep (param f32 f32 f32 i32) (result i32)))
          (memory (export "memory") 1)
          (func (export "io") (result i32) (i32.const 0))
          (func (export "step") (result i32)
            (local $i i32)
            (loop $more
              (drop (call $sweep (f32.const {length}) (f32.const 0) (f32.const 0) (i32.const 128)))
              (local.set $i (i32.add (local.get $i) (i32.const 1)))
              (br_if $more (i32.lt_u (local.get $i) (i32.const {n}))))
            (i32.const 0)))"#
        )
    };
    let mut t = tick();
    step(&sweeps(u32::from(MAX_SWEEPS), 1.0), &mut t).unwrap();
    let error = step(&sweeps(u32::from(MAX_SWEEPS) + 1, 1.0), &mut tick()).unwrap_err();
    assert!(error.contains("sweeps"), "{error}");
    let error = step(&sweeps(1, 9.0), &mut tick()).unwrap_err();
    assert!(error.contains("longer"), "{error}");
}

/// A module that returns nonzero from `step` leaves the tick to the motor.
#[test]
fn a_controller_may_decline_a_tick() {
    let wat = r#"(module
      (memory (export "memory") 1)
      (func (export "io") (result i32) (i32.const 0))
      (func (export "step") (result i32)
        (f32.store (i32.const 28) (f32.const 99))
        (i32.const 1)))"#;
    let mut t = tick();
    step(wat, &mut t).unwrap();
    assert_eq!(t, tick());
}

/// A rough cost, printed for the design document: instantiating fresh
/// every step is the price of purity.
#[test]
fn step_cost() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/samples/sample-wall-kick/controller");
    let controller = compile(&ControllerSource {
        id: "sample-wall-kick:controller/wall-kick".into(),
        module: wat::parse_file(dir.join("wall_kick.wat")).unwrap(),
    })
    .unwrap();
    let mut t = tick();
    t.jump = true;
    let started = std::time::Instant::now();
    let n = 2000;
    for _ in 0..n {
        t.state = ControllerState::default();
        controller.0.step(&mut t, &mut probe()).unwrap();
    }
    eprintln!(
        "wall kick: {:.1} µs a step",
        started.elapsed().as_secs_f64() * 1e6 / f64::from(n)
    );
}
