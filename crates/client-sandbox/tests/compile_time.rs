//! Compile-time bombs: modules shaped to make Cranelift slow. Their shape
//! is bounded at load, and what the bounds allow compiles quickly.
use bri_client_sandbox::{AddOnCode, Budgets, Sandbox, TrustLevel};
use std::time::{Duration, Instant};

/// `count` functions, each a chain of `lines` dependent additions over 500
/// locals: the kind of code register allocation finds hardest.
fn heavy(count: usize, lines: usize) -> Vec<u8> {
    let mut body = String::new();
    for i in 0..lines {
        body.push_str(&format!(
            "(local.set {} (i32.add (local.get {}) (i32.const {i})))\n",
            2 + i % 500,
            2 + (i * 7) % 500
        ));
    }
    let functions: String = (0..count)
        .map(|_| {
            format!(
                "(func (param f32 f32) (local {}) {body})",
                "i32 ".repeat(500)
            )
        })
        .collect();
    wat::parse_str(format!(
        "(module (memory (export \"memory\") 1) {functions})"
    ))
    .unwrap()
}

fn tiny(count: usize) -> Vec<u8> {
    let functions: String = (0..count)
        .map(|i| format!("(func (result i32) (i32.add (i32.const {i}) (i32.const 1)))\n"))
        .collect();
    wat::parse_str(format!(
        "(module (memory (export \"memory\") 1) {functions})"
    ))
    .unwrap()
}

fn folder(module: &[u8]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("client")).unwrap();
    std::fs::write(dir.path().join("client/main.wasm"), module).unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"id":"bomb","version":"1.0.0","client":{"module":"client/main.wasm"}}"#,
    )
    .unwrap();
    dir
}

fn refusal(module: &[u8]) -> String {
    match AddOnCode::load(folder(module).path()) {
        Err(problems) => problems[0].message.clone(),
        Ok(_) => panic!("loaded"),
    }
}

#[test]
fn modules_shaped_to_compile_slowly_are_refused_at_load() {
    // Unbounded, these took 5 s (60,000 tiny functions) and 3 s (one 4 MB
    // function) to compile on one core.
    assert!(refusal(&tiny(60_000)).contains("functions; the limit is"));
    assert!(refusal(&heavy(1, 400_000)).contains("has a function of"));
}

/// Release build: `cargo test --release -p bri-client-sandbox --test
/// compile_time -- --ignored --nocapture`. Measured 2026-09-28 on 4 cores:
/// 0.5 s and 0.7 s.
#[test]
#[ignore = "timing; run in release"]
fn what_the_limits_allow_compiles_in_about_a_second() {
    let sandbox = Sandbox::new().unwrap();
    for (name, module) in [
        ("8 functions at the size limit", heavy(8, 48_000)),
        ("functions at the count limit", tiny(19_990)),
    ] {
        let dir = folder(&module);
        let code = AddOnCode::load(dir.path()).unwrap().unwrap();
        let started = Instant::now();
        sandbox
            .start(&code, Budgets::default(), TrustLevel::Sandboxed)
            .unwrap();
        let took = started.elapsed();
        println!(
            "{name}: {} bytes, compiled and started in {took:?}",
            module.len()
        );
        assert!(took < Duration::from_secs(10), "{name} took {took:?}");
    }
}
