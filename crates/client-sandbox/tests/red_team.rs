//! Hostile Add-Ons and hostile servers against the client sandbox
//! (`docs/audits/red-team.md`). Each test is one attack: it must be stopped
//! with a reason the player can read, and nothing else may be affected.
use bri_client_sandbox::{
    AddOnCode, Budgets, Capability, FrameInput, Sandbox, Stopped, TrustDecision, TrustLevel,
    TrustStore, trust::CodeSummary,
};
use std::path::Path;
use std::time::{Duration, Instant};

fn make(wat: &str, capabilities: &[&str]) -> tempfile::TempDir {
    write_addon(&wat::parse_str(wat).unwrap(), capabilities)
}

fn write_addon(module: &[u8], capabilities: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("client")).unwrap();
    std::fs::write(dir.path().join("client/main.wasm"), module).unwrap();
    let manifest = serde_json::json!({
        "schema_version": 1, "id": "hostile", "version": "1.0.0", "api": 1,
        "name": "Hostile", "license": "CC0-1.0",
        "client": { "module": "client/main.wasm", "capabilities": capabilities }
    });
    std::fs::write(dir.path().join("package.json"), manifest.to_string()).unwrap();
    dir
}

fn load(dir: &Path) -> AddOnCode {
    match AddOnCode::load(dir) {
        Ok(Some(code)) => code,
        other => panic!("{other:?}"),
    }
}

fn frame() -> FrameInput {
    FrameInput {
        dt: 1.0 / 60.0,
        ..Default::default()
    }
}

// ---- Trust ----

fn claimed(hash: &str, capabilities: &[Capability]) -> CodeSummary {
    CodeSummary {
        id: "hostile".into(),
        name: "Hostile".into(),
        code_hash: hash.into(),
        capabilities: capabilities.iter().copied().collect(),
    }
}

/// The prompt is built from what the server says about its code before it
/// downloads. A server that names the real code hash but lists fewer
/// capabilities than the code declares gets a grant for what it showed,
/// and the code that declares more does not run under it.
#[test]
fn a_grant_never_covers_capabilities_the_prompt_did_not_show() {
    let dir = make(
        r#"(module (import "bri" "send" (func (param i32 i32) (result i32)))
            (memory (export "memory") 1))"#,
        &["render.layer", "net.message"],
    );
    let code = load(dir.path());
    let actual = CodeSummary::from(&code);
    let server = "host-key:ab12";

    // What the server claimed: the real hash, but only "draw".
    let lie = claimed(&actual.code_hash, &[Capability::RenderLayer]);
    let mut store = TrustStore::default();
    let TrustDecision::Ask(prompt) = store.decide(server, "Evil", std::slice::from_ref(&lie))
    else {
        panic!("asks")
    };
    assert_eq!(prompt.rows[0].can, ["Draw its own 3D shapes in the world"]);
    store.accept(&prompt, "Evil");

    // The downloaded code declares net.message, which was never shown.
    assert_eq!(store.granted(server, &actual), None);
    assert!(matches!(
        store.decide(server, "Evil", std::slice::from_ref(&actual)),
        TrustDecision::Ask(_)
    ));
    // What was shown is still covered exactly.
    assert_eq!(store.granted(server, &lie), Some(TrustLevel::Sandboxed));
}

/// Code swapped on disk after it was loaded and trusted: what runs is the
/// bytes that were hashed, and loading again sees new code the old grant
/// does not cover.
#[test]
fn code_that_changes_after_the_prompt_is_not_covered_by_it() {
    let dir = make(r#"(module (memory (export "memory") 1))"#, &[]);
    let code = load(dir.path());
    let server = "host-key:ab12";
    let mut store = TrustStore::default();
    let TrustDecision::Ask(prompt) = store.decide(server, "S", &[CodeSummary::from(&code)]) else {
        panic!()
    };
    store.accept(&prompt, "S");

    let evil = wat::parse_str(
        r#"(module (memory (export "memory") 1)
            (func (export "frame") (param f32 f32) (loop $l (br $l))))"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("client/main.wasm"), evil).unwrap();
    // The loaded copy is unchanged and still trusted; it has no frame.
    assert_eq!(
        store.granted(server, &CodeSummary::from(&code)),
        Some(TrustLevel::Sandboxed)
    );
    let mut running = Sandbox::new()
        .unwrap()
        .start(&code, Budgets::default(), TrustLevel::Sandboxed)
        .unwrap();
    assert!(running.frame(frame()).unwrap().draws.is_empty());
    // Loaded again, it is different code and needs asking again.
    let swapped = load(dir.path());
    assert_ne!(swapped.code_hash, code.code_hash);
    assert_eq!(store.granted(server, &CodeSummary::from(&swapped)), None);
}

// ---- Budgets ----

const RECURSE: &str = r#"(module (memory (export "memory") 1)
  (func $down (param i64 i64 i64 i64) (result i64)
    (call $down (i64.add (local.get 0) (i64.const 1)) (local.get 1) (local.get 2) (local.get 3)))
  (func (export "frame") (param f32 f32) (drop (call $down (i64.const 0) (i64.const 0) (i64.const 0) (i64.const 0)))))"#;

/// Use about `kib` KiB of this thread's stack, then run `f`.
#[inline(never)]
fn with_stack_used<R>(kib: usize, f: &mut dyn FnMut() -> R) -> R {
    if kib == 0 {
        return f();
    }
    let pad = std::hint::black_box([0u8; 1024]);
    let result = with_stack_used(kib - 1, f);
    std::hint::black_box(&pad);
    result
}

/// Endless recursion is stopped as a crash, on a thread whose stack is as
/// small as a Windows main thread's (1 MiB) and already partly used, as the
/// game's frame loop is when it runs Add-On code.
#[test]
fn endless_recursion_is_stopped_on_a_small_stack() {
    let dir = make(RECURSE, &[]);
    let code = load(dir.path());
    for used in [0, 256] {
        let code = code.clone();
        let result = std::thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(move || {
                let sandbox = Sandbox::new().unwrap();
                let mut addon = sandbox
                    .start(&code, Budgets::default(), TrustLevel::Sandboxed)
                    .unwrap();
                with_stack_used(used, &mut || addon.frame(frame()).map(|_| ()))
            })
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(
            result,
            Err(Stopped::Crashed("stack overflow".into())),
            "{used} KiB used"
        );
    }
}

/// Tables are bounded like memory.
#[test]
fn growing_a_table_without_end_stops_the_addon() {
    let dir = make(
        r#"(module (memory (export "memory") 1) (table 0 funcref)
            (func (export "frame") (param f32 f32)
              (loop $l (br_if $l (i32.ne (table.grow (ref.null func) (i32.const 50000)) (i32.const -1))))))"#,
        &[],
    );
    let mut addon = Sandbox::new()
        .unwrap()
        .start(&load(dir.path()), Budgets::default(), TrustLevel::Sandboxed)
        .unwrap();
    let error = addon.frame(frame()).unwrap_err();
    assert!(
        matches!(error, Stopped::Memory | Stopped::Crashed(_)),
        "{error:?}"
    );
}

/// One bulk-memory instruction can touch its whole memory while costing a
/// single unit of fuel; the wall-clock deadline still stops a loop of them
/// within about a frame.
#[test]
fn bulk_memory_loops_are_stopped_by_the_clock() {
    let dir = make(
        r#"(module (memory (export "memory") 1024)
            (func (export "frame") (param f32 f32)
              (loop $l
                (memory.fill (i32.const 0) (i32.const 7) (i32.const 67108864))
                (memory.copy (i32.const 0) (i32.const 33554432) (i32.const 33554432))
                (br $l))))"#,
        &[],
    );
    let mut addon = Sandbox::new()
        .unwrap()
        .start(&load(dir.path()), Budgets::default(), TrustLevel::Sandboxed)
        .unwrap();
    let started = Instant::now();
    let error = addon.frame(frame()).unwrap_err();
    let took = started.elapsed();
    assert!(matches!(error, Stopped::Time | Stopped::Cpu), "{error:?}");
    assert!(took < Duration::from_millis(500), "took {took:?}");
}

// ---- Shaders ----

/// A loop that copies a 16 KiB local array every iteration: one expression
/// per copy, but four thousand floats moved. Its cost counts the bytes, so
/// the loop cap (cost x iterations) sees the real work, and enough copies
/// are refused outright.
#[test]
fn copying_large_values_counts_as_the_work_it_is() {
    let copy = |copies: usize| {
        format!(
            "@vertex fn vs_main(v: BriVertex) -> @builtin(position) vec4<f32> {{
                return vec4<f32>(v.position, 1.0);
            }}
            @fragment fn fs_main() -> @location(0) vec4<f32> {{
                var a: array<vec4<f32>, 1024>;
                var b: array<vec4<f32>, 1024>;
                for (var i = 0u; i < 1000000u; i++) {{ {} }}
                return a[7];
            }}",
            "b = a; a = b; ".repeat(copies)
        )
    };
    let one = bri_client_sandbox::shader::compile("copy.wgsl", &copy(1)).unwrap();
    assert!(one.fragment_cost >= 2 * 1024, "cost {}", one.fragment_cost);
    let error = bri_client_sandbox::shader::compile("copy.wgsl", &copy(8)).unwrap_err();
    assert_eq!(error.code, "shader.too_costly");
}
