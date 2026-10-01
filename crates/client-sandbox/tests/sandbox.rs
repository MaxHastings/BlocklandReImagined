//! The client sandbox, headless: the sample Add-On, shader rules, budgets,
//! capabilities, malformed modules and the trust prompt.
use bri_client_sandbox::{
    AddOnCode, Budgets, Capability, FrameInput, Sandbox, Stopped, TrustDecision, TrustLevel,
    TrustStore, shader, trust::CodeSummary,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/samples/spinning-cube")
}

/// An Add-On folder with the given module (WebAssembly text), capabilities
/// and shaders.
fn make(wat: &str, capabilities: &[&str], shaders: &[(&str, &str)]) -> tempfile::TempDir {
    addon_bytes(&wat::parse_str(wat).unwrap(), capabilities, shaders)
}

fn addon_bytes(
    module: &[u8],
    capabilities: &[&str],
    shaders: &[(&str, &str)],
) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("client")).unwrap();
    std::fs::write(dir.path().join("client/main.wasm"), module).unwrap();
    for (name, source) in shaders {
        std::fs::write(dir.path().join(name), source).unwrap();
    }
    let manifest = serde_json::json!({
        "schema_version": 1, "id": "test-addon", "version": "1.0.0", "api": 1,
        "name": "Test Add-On", "license": "CC0-1.0",
        "client": {
            "module": "client/main.wasm",
            "capabilities": capabilities,
            "shaders": shaders.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        }
    });
    std::fs::write(dir.path().join("package.json"), manifest.to_string()).unwrap();
    dir
}

fn load(dir: &Path) -> AddOnCode {
    match AddOnCode::load(dir) {
        Ok(Some(code)) => code,
        Ok(None) => panic!("no client code"),
        Err(problems) => panic!("{problems:#?}"),
    }
}

fn load_error(dir: &Path) -> Vec<String> {
    match AddOnCode::load(dir) {
        Err(problems) => problems.into_iter().map(|p| p.code).collect(),
        Ok(_) => panic!("loaded"),
    }
}

fn start(code: &AddOnCode, budgets: Budgets) -> Result<bri_client_sandbox::AddOn, Stopped> {
    Sandbox::new()
        .unwrap()
        .start(code, budgets, TrustLevel::Sandboxed)
}

/// Budgets for tests of anything but wall-clock time. Instructions (fuel)
/// still bound every call and the GPU budgets keep their defaults, but no
/// wall-clock limit can fire first on a loaded machine: the default 8 ms
/// frame is a few scheduler slices, so a busy PC turns a draw or memory
/// refusal into `Stopped::Time`.
fn budgets() -> Budgets {
    let timed = Budgets::default();
    Budgets {
        gpu_ms_per_frame: timed.gpu_ms_per_frame,
        gpu_strikes: timed.gpu_strikes,
        gpu_stop_ms: timed.gpu_stop_ms,
        ..Budgets::untimed()
    }
}

fn frame(t: f32) -> FrameInput {
    FrameInput {
        time: t,
        dt: 1.0 / 60.0,
        ..Default::default()
    }
}

const SAMPLE_SHADER: &str =
    include_str!("../../../packages/samples/spinning-cube/client/cube.wgsl");

// ---- The sample Add-On ----

#[test]
fn the_sample_module_is_built_from_its_source() {
    let dir = sample_dir();
    let built = wat::parse_file(dir.join("client/main.wat")).unwrap();
    let path = dir.join("client/main.wasm");
    if std::env::var_os("BRI_BLESS").is_some() {
        std::fs::write(&path, &built).unwrap();
    }
    assert_eq!(
        std::fs::read(&path).unwrap(),
        built,
        "client/main.wasm is stale; rerun with BRI_BLESS=1"
    );
}

#[test]
fn the_sample_draws_a_cube_with_its_own_shader() {
    let code = load(&sample_dir());
    assert_eq!(code.name, "Spinning Cube");
    assert_eq!(
        code.capabilities.iter().copied().collect::<Vec<_>>(),
        [Capability::RenderLayer, Capability::RenderShader]
    );
    assert_eq!(code.shaders.len(), 1);
    assert_eq!(code.shaders[0].loops, 1, "the ring loop is bounded");

    let mut addon = start(&code, budgets()).unwrap();
    assert_eq!(addon.layer().meshes.len(), 1);
    assert_eq!(addon.layer().meshes[0].vertices.len(), 24);
    assert_eq!(
        addon.layer().materials[0].params[0],
        [0.95, 0.55, 0.15, 1.0]
    );

    let first = addon.frame(frame(0.0)).unwrap().clone();
    assert_eq!(first.draws.len(), 1);
    assert_eq!(
        first.log,
        ["spinning cube ready"],
        "init's log arrives with frame one"
    );
    assert_eq!(first.triangles, 12);
    let second = addon.frame(frame(0.5)).unwrap().clone();
    assert_ne!(
        first.draws[0].model[13], second.draws[0].model[13],
        "the cube bobs"
    );
    assert!(second.log.is_empty());
}

/// Needs a GPU: renders the sample and checks the cube is there and moves.
#[test]
#[ignore = "needs a GPU adapter"]
fn the_sample_renders_offscreen_and_animates() {
    let code = load(&sample_dir());
    // What it draws, not how fast: a loaded machine's slow frame must not
    // stop it.
    let mut addon = start(&code, Budgets::untimed()).unwrap();
    let (adapter, images) =
        bri_client_sandbox::gpu::render_offscreen(&mut addon, 256, 192, &[0.0, 0.75]).unwrap();
    let centre = |i: usize| {
        let image = &images[i];
        let at = ((image.height / 2 * image.width + image.width / 2) * 4) as usize;
        image.pixels[at..at + 4].to_vec()
    };
    let background = images[0].pixels[0..4].to_vec();
    assert_ne!(
        centre(0),
        background,
        "the cube covers the centre on {adapter}"
    );
    assert_ne!(images[0].pixels, images[1].pixels, "the shader animates");
}

// ---- Shaders ----

fn shader_error(source: &str) -> String {
    shader::compile("test.wgsl", source)
        .unwrap_err()
        .code
        .to_string()
}

const PASS: &str = "
@vertex fn vs_main(v: BriVertex) -> @builtin(position) vec4<f32> {
    return bri_frame.view_proj * bri_draw.model * vec4<f32>(v.position, 1.0);
}
@fragment fn fs_main() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }
";

#[test]
fn a_shader_is_validated_against_the_interface() {
    assert!(shader::compile("pass.wgsl", PASS).is_ok());
    assert!(shader::compile("cube.wgsl", SAMPLE_SHADER).is_ok());
    // The engine sets the loop allowance every frame, low until it has
    // measured the GPU.
    assert!(shader::PRELUDE.contains("limits: vec4<u32>"));
    const { assert!(shader::DEFAULT_LOOP_LIMIT <= 16) };
    const { assert!(shader::DEFAULT_LOOP_LIMIT < shader::MAX_LOOP_LIMIT) };
    let pass = shader::compile("pass.wgsl", PASS).unwrap();
    assert!(pass.fragment_cost > 0 && pass.fragment_cost < 64);

    // Error locations are the Add-On's own lines.
    let error = shader::compile("bad.wgsl", "\n\nfn broken( {").unwrap_err();
    assert_eq!(error.code, "shader.parse");
    assert!(error.message.contains("bad.wgsl:3"), "{}", error.message);

    assert_eq!(
        shader_error("@fragment fn fs_main() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }"),
        "shader.entry_point"
    );
    assert_eq!(
        shader_error(&format!(
            "{PASS}\n@compute @workgroup_size(64) fn boom() {{}}"
        )),
        "shader.entry_point"
    );
    assert_eq!(
        shader_error(
            &PASS
                .replace(
                    "vec4<f32>(v.position, 1.0)",
                    "vec4<f32>(v.position, 1.0) + extra.x"
                )
                .replace(
                    "@vertex fn vs_main(v: BriVertex)",
                    "@vertex fn vs_main(v: BriVertex, @location(5) extra: vec4<f32>)"
                )
        ),
        "shader.vertex_input"
    );
    assert_eq!(
        shader_error(&PASS.replace("-> @location(0) vec4<f32>", "-> @location(1) vec4<f32>")),
        "shader.fragment_output"
    );
}

#[test]
fn hostile_shaders_are_refused() {
    // Reading or writing anything but its own draw.
    let storage = format!(
        "@group(2) @binding(0) var<storage, read_write> loot: array<u32>;\n{}",
        PASS.replace(
            "return vec4<f32>(1.0);",
            "loot[0] = 1u; return vec4<f32>(1.0);"
        )
    );
    assert_eq!(shader_error(&storage), "shader.resource");
    let texture = format!(
        "@group(2) @binding(0) var screen: texture_2d<f32>;\n{}",
        PASS.replace(
            "return vec4<f32>(1.0);",
            "return textureLoad(screen, vec2<i32>(0), 0);"
        )
    );
    assert_eq!(shader_error(&texture), "shader.resource");
    // Resetting the engine's loop allowance.
    let reset = PASS.replace(
        "return vec4<f32>(1.0);",
        "bri_loop_budget = 99999999u; return vec4<f32>(1.0);",
    );
    assert_eq!(shader_error(&reset), "shader.reserved");
    // A local array too large for any GPU's registers.
    let huge = PASS.replace(
        "return vec4<f32>(1.0);",
        "var a: array<vec4<f32>, 100000>; a[3] = vec4<f32>(1.0); return a[3];",
    );
    assert_eq!(shader_error(&huge), "shader.type_too_large");
    // Pipeline constants the engine does not set.
    assert_eq!(
        shader_error(&format!("override knob: u32 = 1u;\n{PASS}")),
        "shader.override"
    );
    // No loops, but each helper calls the one before twice: 2^20 times the
    // work of the first.
    let mut fanout = String::from("fn f0(x: f32) -> f32 { return sin(x); }\n");
    for i in 1..=20 {
        fanout += &format!(
            "fn f{i}(x: f32) -> f32 {{ return f{j}(x) + f{j}(x + 1.0); }}\n",
            j = i - 1
        );
    }
    fanout += &PASS.replace(
        "return vec4<f32>(1.0);",
        "return vec4<f32>(f20(bri_frame.time.x));",
    );
    assert_eq!(shader_error(&fanout), "shader.too_costly");
    // A source the size of a small novel.
    assert_eq!(
        shader_error(&format!("{PASS}{}", "//".repeat(40_000))),
        "shader.too_large"
    );
}

#[test]
fn every_loop_is_bounded_even_one_that_never_ends() {
    let forever = PASS.replace(
        "return vec4<f32>(1.0);",
        "var x = 0.0; loop { x += 1.0; } return vec4<f32>(x);",
    );
    let bounded = shader::compile("forever.wgsl", &forever).unwrap();
    assert_eq!(bounded.loops, 1);
    // The rewrite put the allowance check first in the loop body.
    let fragment = bounded
        .module
        .entry_points
        .iter()
        .find(|e| e.name == "fs_main")
        .unwrap();
    let body = fragment
        .function
        .body
        .iter()
        .find_map(|s| match s {
            naga::Statement::Loop { body, .. } => Some(body),
            _ => None,
        })
        .unwrap();
    assert!(matches!(body[1], naga::Statement::If { .. }));
    // And every entry point starts by loading the allowance for this frame.
    assert!(matches!(
        fragment.function.body[1],
        naga::Statement::Store { .. }
    ));

    // Nested loops and loops in helper functions draw on the same budget.
    let nested = format!(
        "fn helper() -> f32 {{ var s = 0.0; for (var i = 0; i < 100000; i++) {{ s += 1.0; }} return s; }}\n{}",
        PASS.replace(
            "return vec4<f32>(1.0);",
            "var t = 0.0; while (true) { for (var j = 0; j < 1000000; j++) { t += helper(); } } return vec4<f32>(t);"
        )
    );
    assert_eq!(shader::compile("nested.wgsl", &nested).unwrap().loops, 3);
}

// ---- Budgets ----

const SPIN_IN_FRAME: &str = r#"(module
  (memory (export "memory") 1)
  (func (export "frame") (param f32 f32) (loop $l (br $l))))"#;

#[test]
fn a_frame_that_never_returns_is_stopped_and_the_rest_carries_on() {
    let dir = make(SPIN_IN_FRAME, &[], &[]);
    // No wall-clock budget, so fuel is what stops it, on any machine.
    let mut addon = start(&load(dir.path()), budgets()).unwrap();
    assert_eq!(addon.frame(frame(0.0)).unwrap_err(), Stopped::Cpu);
    // Stopped for good, and says why every time.
    assert_eq!(addon.frame(frame(1.0)).unwrap_err(), Stopped::Cpu);
    assert!(addon.stopped().is_some());
}

#[test]
fn wall_clock_time_is_a_budget_too() {
    let dir = make(SPIN_IN_FRAME, &[], &[]);
    let budgets = Budgets {
        frame_fuel: u64::MAX / 2,
        frame_time: Duration::from_millis(20),
        ..budgets()
    };
    let mut addon = start(&load(dir.path()), budgets).unwrap();
    let started = std::time::Instant::now();
    assert_eq!(addon.frame(frame(0.0)).unwrap_err(), Stopped::Time);
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn init_and_start_run_under_budget() {
    let spin_start = r#"(module (memory (export "memory") 1)
        (func $s (loop $l (br $l))) (start $s))"#;
    let dir = make(spin_start, &[], &[]);
    let budgets = Budgets {
        init_time: Duration::from_secs(60),
        ..Budgets::default()
    };
    assert_eq!(start(&load(dir.path()), budgets).err(), Some(Stopped::Cpu));
}

#[test]
fn memory_past_the_budget_stops_the_addon() {
    let grow = r#"(module (memory (export "memory") 1)
        (func (export "frame") (param f32 f32)
          (drop (memory.grow (i32.const 2048)))))"#;
    let dir = make(grow, &[], &[]);
    let mut addon = start(&load(dir.path()), budgets()).unwrap();
    assert_eq!(addon.frame(frame(0.0)).unwrap_err(), Stopped::Memory);

    // A module that asks for more up front never starts.
    let big = r#"(module (memory (export "memory") 2048))"#;
    let dir = make(big, &[], &[]);
    assert_eq!(
        start(&load(dir.path()), budgets()).err(),
        Some(Stopped::Memory)
    );
}

const DRAW_LOOP: &str = r#"(module
  (import "bri" "shader" (func $shader (param i32 i32) (result i32)))
  (import "bri" "mesh_create" (func $mesh (param i32 i32 i32 i32) (result i32)))
  (import "bri" "material_create" (func $material (param i32) (result i32)))
  (import "bri" "draw" (func $draw (param i32 i32 i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "pass.wgsl")
  ;; One triangle at 64: three vertices of zeros, indices 0 1 2 at 256.
  (data (i32.const 256) "\00\00\00\00\01\00\00\00\02\00\00\00")
  (data (i32.const 512) "\00\00\80\3f")
  (func (export "init")
    (drop (call $mesh (i32.const 64) (i32.const 3) (i32.const 256) (i32.const 3)))
    (drop (call $material (call $shader (i32.const 0) (i32.const 9)))))
  (func (export "frame") (param f32 f32)
    (local $i i32)
    (loop $l
      (call $draw (i32.const 0) (i32.const 0) (i32.const MATRIX))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br_if $l (i32.lt_u (local.get $i) (i32.const COUNT))))))"#;

fn draws(count: u32, matrix: u32) -> tempfile::TempDir {
    let wat = DRAW_LOOP
        .replace("COUNT", &count.to_string())
        .replace("MATRIX", &matrix.to_string());
    make(
        &wat,
        &["render.layer", "render.shader"],
        &[("pass.wgsl", PASS)],
    )
}

#[test]
fn host_requests_have_budgets() {
    let dir = draws(10, 512);
    let mut addon = start(&load(dir.path()), budgets()).unwrap();
    assert_eq!(addon.frame(frame(0.0)).unwrap().draws.len(), 10);

    let dir = draws(100_000, 512);
    let mut addon = start(&load(dir.path()), budgets()).unwrap();
    assert!(matches!(addon.frame(frame(0.0)), Err(Stopped::Budget(_))));
    // What it held is gone.
    assert!(addon.layer().meshes.is_empty());
}

#[test]
fn a_pointer_outside_its_memory_is_misuse() {
    let dir = draws(1, 65_530);
    let mut addon = start(&load(dir.path()), budgets()).unwrap();
    match addon.frame(frame(0.0)) {
        Err(Stopped::Misuse(why)) => assert!(why.contains("outside its memory"), "{why}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_gpu_budget_allows_spikes_but_not_a_sustained_overrun() {
    let dir = draws(1, 512);
    let mut addon = start(&load(dir.path()), budgets()).unwrap();
    for _ in 0..19 {
        addon.report_gpu_time(20.0).unwrap();
    }
    addon.report_gpu_time(1.0).unwrap(); // A good frame resets the count.
    for _ in 0..19 {
        addon.report_gpu_time(20.0).unwrap();
    }
    assert!(matches!(addon.report_gpu_time(20.0), Err(Stopped::Gpu(_))));
}

#[test]
fn one_frame_far_over_the_gpu_budget_stops_the_addon_at_once() {
    let dir = draws(1, 512);
    let mut addon = start(&load(dir.path()), budgets()).unwrap();
    addon.report_gpu_time(60.0).unwrap(); // A spike is allowed.
    match addon.report_gpu_time(2400.0) {
        Err(Stopped::Gpu(why)) => assert!(why.contains("2400 ms"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert!(addon.stopped().is_some());
    assert!(addon.frame(frame(0.0)).is_err());
}

#[test]
fn the_shader_loop_cap_starts_low_and_fits_the_measured_gpu() {
    use bri_client_sandbox::gpu::{GpuSpeed, loop_limit};
    let cost = [20, 60];
    let four_k = 3840 * 2160;
    // Not measured yet: the low default, whatever the screen.
    assert_eq!(
        loop_limit(None, cost, four_k, 36, 4.0, 1.0),
        shader::DEFAULT_LOOP_LIMIT
    );
    // The verifier's RTX 4070 SUPER: the heavy shader ran 4096 iterations
    // over 1920x1080 in about 2.4 s.
    let fast = GpuSpeed {
        work_per_ms: 1920.0 * 1080.0 * 60.0 * 4097.0 / 2400.0,
    };
    let at_4k = loop_limit(Some(fast), cost, four_k, 36, 4.0, 1.0);
    let at_1080p = loop_limit(Some(fast), cost, 1920 * 1080, 36, 4.0, 1.0);
    // 4 ms of a 2400 ms frame, over twice the pixels (overdraw 2).
    assert!((1..8).contains(&at_1080p), "{at_1080p}");
    assert!(at_4k < at_1080p, "{at_4k} {at_1080p}");
    // Slow frames scale it down; it never exceeds the maximum.
    assert!(loop_limit(Some(fast), cost, four_k, 36, 4.0, 0.25) < at_4k.max(1));
    let tiny = loop_limit(Some(fast), cost, 64, 3, 4.0, 1.0);
    assert_eq!(tiny, shader::MAX_LOOP_LIMIT);
    // A GPU far slower than the work: no loop iterations at all.
    let slow = GpuSpeed { work_per_ms: 1.0 };
    assert_eq!(loop_limit(Some(slow), cost, four_k, 36, 4.0, 1.0), 0);
}

// ---- Capabilities ----

#[test]
fn an_undeclared_capability_is_denied_before_anything_runs() {
    let wat = r#"(module
      (import "bri" "sound_play" (func (param i32 i32 f32) (result i32)))
      (memory (export "memory") 1))"#;
    let dir = make(wat, &["render.layer"], &[]);
    assert_eq!(load_error(dir.path()), ["client.capability.denied"]);

    // Declared, it loads.
    let dir = make(wat, &["audio"], &[]);
    assert!(start(&load(dir.path()), budgets()).is_ok());
}

#[test]
fn elevated_code_needs_full_trust_and_native_is_not_built_yet() {
    let empty = r#"(module (memory (export "memory") 1))"#;
    let sandbox = Sandbox::new().unwrap();
    for capability in ["net.http", "native"] {
        let dir = make(empty, &[capability], &[]);
        let code = load(dir.path());
        assert_eq!(code.tier(), bri_client_sandbox::Tier::Elevated);
        match sandbox.start(&code, Budgets::default(), TrustLevel::Sandboxed) {
            Err(Stopped::Misuse(why)) => assert!(why.contains("full trust"), "{why}"),
            other => panic!("{:?}", other.err()),
        }
        // Fully trusted, it is still refused until the capability exists.
        match sandbox.start(&code, Budgets::default(), TrustLevel::Elevated) {
            Err(Stopped::Misuse(why)) => assert!(why.contains("cannot run yet"), "{why}"),
            other => panic!("{:?}", other.err()),
        }
    }
}

#[test]
fn a_native_plugin_needs_the_server_name_typed() {
    let store = TrustStore::default();
    let plugin = CodeSummary {
        id: "physics-plus".into(),
        name: "Physics Plus".into(),
        code_hash: "dddd".into(),
        capabilities: [Capability::Native].into_iter().collect(),
    };
    let TrustDecision::Ask(prompt) = store.decide("host-key:ab12", "Lab", &[plugin]) else {
        panic!()
    };
    assert_eq!(prompt.level, TrustLevel::Elevated);
    assert_eq!(prompt.type_to_confirm.as_deref(), Some("Lab"));
    assert!(prompt.footer.contains("no sandbox limits it"));
    assert_eq!(
        prompt.rows[0].can,
        ["Run as a normal program with full access to your PC"]
    );
    // Sandboxed and other elevated prompts need no typing.
    let TrustDecision::Ask(web) = store.decide(
        "host-key:ab12",
        "Lab",
        &[summary("eeee", &[Capability::NetHttp])],
    ) else {
        panic!()
    };
    assert_eq!(web.type_to_confirm, None);
}

#[test]
fn keys_are_only_readable_while_focused() {
    let wat = r#"(module
      (import "bri" "key_down" (func $key (param i32) (result i32)))
      (import "bri" "send" (func $send (param i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "frame") (param f32 f32)
        (i32.store8 (i32.const 0) (i32.add (i32.const 48) (call $key (i32.const 87))))
        (drop (call $send (i32.const 0) (i32.const 1)))))"#;
    let dir = make(wat, &["input.focused", "net.message"], &[]);
    let mut addon = start(&load(dir.path()), budgets()).unwrap();
    let pressed = |focused| FrameInput {
        focused,
        keys_down: vec![87],
        ..Default::default()
    };
    assert_eq!(addon.frame(pressed(false)).unwrap().outbox, [b"0".to_vec()]);
    assert_eq!(addon.frame(pressed(true)).unwrap().outbox, [b"1".to_vec()]);
}

#[test]
fn messages_come_from_its_own_server_script_in_order() {
    // Echo every message back, reversed in order of arrival: none lost.
    let wat = r#"(module
      (import "bri" "recv" (func $recv (param i32 i32) (result i32)))
      (import "bri" "send" (func $send (param i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "frame") (param f32 f32)
        (local $n i32)
        (block $done (loop $l
          (local.set $n (call $recv (i32.const 0) (i32.const 1024)))
          (br_if $done (i32.lt_s (local.get $n) (i32.const 0)))
          (drop (call $send (i32.const 0) (local.get $n)))
          (br $l)))))"#;
    let dir = make(wat, &["net.message"], &[]);
    let mut addon = start(&load(dir.path()), budgets()).unwrap();
    let out = addon
        .frame(FrameInput {
            messages: vec![b"dig".to_vec(), b"crack".to_vec()],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(out.outbox, [b"dig".to_vec(), b"crack".to_vec()]);
}

// ---- Malformed modules ----

#[test]
fn malformed_modules_are_rejected_before_compiling() {
    let dir = addon_bytes(b"\0asm\x01\0\0\0garbage", &[], &[]);
    assert_eq!(load_error(dir.path()), ["client.module.malformed"]);
    let dir = addon_bytes(b"MZ\x90\0 a Windows program", &[], &[]);
    assert_eq!(load_error(dir.path()), ["client.module.malformed"]);

    // Asking the host for a memory, or for WASI (files, clocks, sockets).
    let dir = make(r#"(module (import "bri" "memory" (memory 1)))"#, &[], &[]);
    assert_eq!(load_error(dir.path()), ["client.module.malformed"]);
    let wasi = r#"(module (import "wasi_snapshot_preview1" "fd_write" (func (param i32 i32 i32 i32) (result i32))) (memory (export "memory") 1))"#;
    let dir = make(wasi, &[], &[]);
    assert_eq!(load_error(dir.path()), ["client.import.unknown"]);

    // Threads and shared memory are outside the allowed feature set.
    let dir = make(
        r#"(module (memory (export "memory") 1 1 shared))"#,
        &[],
        &[],
    );
    assert_eq!(load_error(dir.path()), ["client.module.malformed"]);

    // Valid, but exports no memory for the host to read.
    let dir = make(r#"(module)"#, &[], &[]);
    assert!(matches!(
        start(&load(dir.path()), budgets()),
        Err(Stopped::Misuse(_))
    ));
}

#[test]
fn files_outside_the_addon_are_refused() {
    let dir = make(r#"(module (memory (export "memory") 1))"#, &[], &[]);
    let manifest = serde_json::json!({
        "schema_version": 1, "id": "test-addon", "version": "1.0.0", "api": 1,
        "name": "x", "license": "CC0-1.0",
        "client": { "module": "../../secret.wasm", "capabilities": [] }
    });
    std::fs::write(dir.path().join("package.json"), manifest.to_string()).unwrap();
    assert_eq!(load_error(dir.path()), ["client.path"]);
}

// ---- Trust ----

fn summary(hash: &str, capabilities: &[Capability]) -> CodeSummary {
    CodeSummary {
        id: "spinning-cube".into(),
        name: "Spinning Cube".into(),
        code_hash: hash.into(),
        capabilities: capabilities.iter().copied().collect(),
    }
}

#[test]
fn joining_asks_once_per_server_and_again_when_the_code_changes() {
    let mut store = TrustStore::default();
    let server = "host-key:ab12";
    assert_eq!(
        store.decide(server, "Max's Server", &[]),
        TrustDecision::Join
    );

    let v1 = [summary(
        "aaaa",
        &[Capability::RenderLayer, Capability::RenderShader],
    )];
    let TrustDecision::Ask(prompt) = store.decide(server, "Max's Server", &v1) else {
        panic!("asks the first time");
    };
    assert_eq!(prompt.level, TrustLevel::Sandboxed);
    assert_eq!(prompt.title, "Max's Server wants to run Add-On code");
    assert_eq!((prompt.accept, prompt.decline), ("Trust and join", "Leave"));
    assert_eq!(
        prompt.rows[0].can,
        [
            "Draw its own 3D shapes in the world",
            "Use its own graphics effects (shaders)"
        ]
    );
    assert!(prompt.footer.contains("cannot read your files"));
    store.accept(&prompt, "Max's Server");
    assert_eq!(
        store.decide(server, "Max's Server", &v1),
        TrustDecision::Join
    );
    assert_eq!(store.granted(server, &v1[0]), Some(TrustLevel::Sandboxed));

    // Another server asks for itself.
    assert!(matches!(
        store.decide("host-key:ffff", "Other", &v1),
        TrustDecision::Ask(_)
    ));

    // New code asks again and says it changed; the old grant does not
    // cover it.
    let v2 = [summary(
        "bbbb",
        &[Capability::RenderLayer, Capability::RenderShader],
    )];
    let TrustDecision::Ask(prompt) = store.decide(server, "Max's Server", &v2) else {
        panic!("asks again");
    };
    assert!(prompt.rows[0].changed);
    assert_eq!(store.granted(server, &v2[0]), None);

    // Saved and loaded, and revocable.
    let dir = tempfile::tempdir().unwrap();
    store.save(dir.path()).unwrap();
    let mut loaded = TrustStore::load(dir.path()).unwrap();
    assert_eq!(loaded, store);
    assert!(loaded.revoke_server(server));
    assert!(matches!(
        loaded.decide(server, "Max's Server", &v1),
        TrustDecision::Ask(_)
    ));
}

#[test]
fn going_beyond_the_sandbox_is_a_separate_stronger_choice() {
    let mut store = TrustStore::default();
    let server = "host-key:ab12";
    let mut theatre = summary("cccc", &[Capability::RenderLayer, Capability::NetHttp]);
    theatre.id = "theatre".into();
    theatre.name = "Movie Theatre".into();
    let cube = summary("aaaa", &[Capability::RenderLayer]);
    let code = [cube.clone(), theatre.clone()];

    // The sandboxed Add-On is asked about first, on its own.
    let TrustDecision::Ask(first) = store.decide(server, "Cinema", &code) else {
        panic!()
    };
    assert_eq!(first.level, TrustLevel::Sandboxed);
    assert_eq!(first.rows.len(), 1);
    store.accept(&first, "Cinema");

    // Then the elevated one, with the risk spelled out and a box to tick.
    let TrustDecision::Ask(second) = store.decide(server, "Cinema", &code) else {
        panic!()
    };
    assert_eq!(second.level, TrustLevel::Elevated);
    assert_eq!(second.rows[0].id, "theatre");
    assert!(second.rows[0].can.iter().any(|c| c.contains("IP address")));
    assert!(second.confirm.as_deref().unwrap().contains("I trust them"));
    assert_eq!(second.decline, "Join without them");
    store.accept(&second, "Cinema");
    assert_eq!(store.decide(server, "Cinema", &code), TrustDecision::Join);
    assert_eq!(store.granted(server, &theatre), Some(TrustLevel::Elevated));

    // Revoking one Add-On leaves the other trusted.
    assert!(store.revoke_addon(server, "theatre"));
    assert_eq!(store.granted(server, &cube), Some(TrustLevel::Sandboxed));
    assert_eq!(store.granted(server, &theatre), None);
}

#[test]
fn a_sandboxed_grant_never_covers_elevated_code() {
    let mut store = TrustStore::default();
    let server = "host-key:ab12";
    let v1 = [summary("aaaa", &[Capability::RenderLayer])];
    let TrustDecision::Ask(prompt) = store.decide(server, "S", &v1) else {
        panic!()
    };
    store.accept(&prompt, "S");
    // Same Add-On, now asking for the internet: never silently allowed.
    let v2 = [summary(
        "aaaa",
        &[Capability::RenderLayer, Capability::NetHttp],
    )];
    assert_eq!(store.granted(server, &v2[0]), None);
    let TrustDecision::Ask(prompt) = store.decide(server, "S", &v2) else {
        panic!()
    };
    assert_eq!(prompt.level, TrustLevel::Elevated);
}

/// Needs a GPU: a fragment shader that loops forever, drawn over the whole
/// screen, finishes because the loop is bounded, and runs exactly the cap
/// the renderer fitted. The GPU's speed is given rather than measured, so
/// the cap is the same on a loaded machine; fitting caps to measured
/// speeds and slow frames is checked without a GPU
/// (`the_shader_loop_cap_starts_low_and_fits_the_measured_gpu`).
#[test]
#[ignore = "needs a GPU adapter"]
fn an_endless_shader_loop_finishes_on_the_gpu() {
    use bri_client_sandbox::gpu::{GpuSpeed, loop_limit, render_offscreen_at_speed};
    let endless = "
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.5, 1.0);
}
@fragment fn fs_main(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    var x = at.x;
    loop { x = fract(sin(x) * 43758.5453); }
    return vec4<f32>(x, 0.0, 0.0, 1.0);
}";
    let wat = DRAW_LOOP.replace("COUNT", "1").replace("MATRIX", "512");
    let dir = make(
        &wat,
        &["render.layer", "render.shader"],
        &[("pass.wgsl", endless)],
    );
    let code = load(dir.path());
    let cost = [code.shaders[0].vertex_cost, code.shaders[0].fragment_cost];
    let (pixels, vertices) = (512 * 512, 3);
    let budget = budgets().gpu_ms_per_frame;
    // The speed at which `cap` iterations fill the frame budget.
    let speed = |cap: u32| GpuSpeed {
        work_per_ms: (pixels as f64 * bri_client_sandbox::gpu::OVERDRAW * f64::from(cost[1])
            + f64::from(vertices as u32 * cost[0]))
            * (f64::from(cap) + 1.5)
            / f64::from(budget),
    };
    let render = |cap: u32| {
        let speed = speed(cap);
        assert_eq!(
            loop_limit(Some(speed), cost, pixels, vertices, budget, 1.0),
            cap
        );
        let mut addon = start(&code, budgets()).unwrap();
        let (adapter, images) =
            render_offscreen_at_speed(&mut addon, 512, 512, &[0.0, 0.1, 0.2], speed).unwrap();
        println!("{adapter}: 512x512 at a loop cap of {cap}");
        // Every frame finished, with the loop capped where it was fitted.
        assert_eq!(images.len(), 3);
        for image in &images {
            assert_eq!(image.loop_limit, cap);
            assert_eq!(image.gpu_ms, None, "the layer is not timed");
        }
        images
    };
    let low = render(shader::DEFAULT_LOOP_LIMIT);
    let high = render(shader::DEFAULT_LOOP_LIMIT * 4);
    // The cap reaches the shader: more iterations, a different picture.
    assert_ne!(low[0].pixels, high[0].pixels);
}

/// Needs a GPU: calibration measures a speed (and, as a benchmark, takes
/// well under a second).
#[test]
#[ignore = "needs a GPU adapter"]
fn calibration_measures_the_gpu_quickly() {
    let (adapter, device, queue) = bri_client_sandbox::gpu::headless_device().unwrap();
    let started = std::time::Instant::now();
    let speed = bri_client_sandbox::gpu::calibrate(&device, &queue).unwrap();
    println!(
        "{adapter}: {:.3e} shader expressions per ms, measured in {:?}",
        speed.work_per_ms,
        started.elapsed()
    );
    assert!(speed.work_per_ms > 0.0);
    // Wall time depends on the machine and its load: a benchmark check.
    if std::env::var_os("BRI_BENCH").is_some() {
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
