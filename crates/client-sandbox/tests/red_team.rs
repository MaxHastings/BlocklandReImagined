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
