//! The Add-On bundle (`tools/addon_bundle.py`): a release carries classic
//! Add-Ons as their authors' originals, imported with their port and
//! credited, while the repository lists only their names, authors and the
//! hashes of the copies it may bundle. Run on the synthetic shotgun the
//! port tests use, in a stand-in checkout, so it needs no original.
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ports")
}

fn python() -> &'static str {
    ["python3", "python"]
        .into_iter()
        .find(|p| {
            Command::new(p)
                .arg("--version")
                .output()
                .is_ok_and(|o| o.status.success())
        })
        .expect("Python 3 runs tools/addon_bundle.py")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bri-bundle-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn read(path: &Path) -> Value {
    serde_json::from_slice(
        &std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
    )
    .unwrap()
}

/// A stand-in checkout: the default list given, the real ports list, a
/// v20 folder with nothing in it, an empty core script and generated game
/// content with no packs (what the imports are run against).
struct Checkout {
    root: PathBuf,
}
impl Checkout {
    fn new(name: &str, addons: Value) -> Self {
        let root = scratch(name);
        std::fs::create_dir_all(root.join("packages")).unwrap();
        std::fs::write(
            root.join("packages/default-addons.json"),
            serde_json::to_vec_pretty(&json!({ "schema_version": 2, "addons": addons })).unwrap(),
        )
        .unwrap();
        std::fs::create_dir_all(root.join("crates/addon-import/ports")).unwrap();
        std::fs::copy(
            repo().join("crates/addon-import/ports/ports.json"),
            root.join("crates/addon-import/ports/ports.json"),
        )
        .unwrap();
        std::fs::create_dir_all(root.join("v20/base")).unwrap();
        std::fs::create_dir_all(root.join("v20/Add-Ons")).unwrap();
        std::fs::write(root.join("core.cs"), "").unwrap();
        std::fs::create_dir_all(root.join("game")).unwrap();
        std::fs::write(
            root.join("game/packages.json"),
            r#"{ "schema_version": 1, "packages": [] }"#,
        )
        .unwrap();
        Self { root }
    }
    fn run(&self, args: &[&str]) -> Output {
        let root = self.root.to_str().unwrap();
        let mut command = Command::new(python());
        command
            .arg(repo().join("tools/addon_bundle.py"))
            .args(args)
            .args(["--repo", root])
            .env_remove("BRI_ADDON_SEARCH")
            .env_remove("BRI_V20");
        if matches!(args[0], "build" | "find") {
            command.args([
                "--search",
                fixtures().to_str().unwrap(),
                "--v20",
                &format!("{root}/v20"),
                "--core",
                &format!("{root}/core.cs"),
                "--importer",
                env!("CARGO_BIN_EXE_bri-import-addon"),
                "--out",
                &format!("{root}/bundle"),
                "--content-root",
                &format!("{root}/game"),
            ]);
        }
        command.output().unwrap()
    }
}
impl Drop for Checkout {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The hash Import Add-On records for the synthetic shotgun.
fn shotgun_sha() -> String {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = scratch(&format!("sha-{n}")).join("weapon_shotgun");
    let report = bri_addon_import::import(&bri_addon_import::Options {
        input: fixtures().join("Weapon_Shotgun"),
        out: out.clone(),
        ..Default::default()
    })
    .unwrap();
    let _ = std::fs::remove_dir_all(out.parent().unwrap());
    report.source.sha256
}

fn shotgun(sha256: &[&str], extra: Value) -> Value {
    let mut entry = json!({
        "id": "weapon_shotgun",
        "enabled": false,
        "original": {
            "addon": "Weapon_Shotgun",
            "title": "Sawn-off Shotgun",
            "authors": ["Someone", "Someone Else"],
            "version": "1.2.0",
            "sha256": sha256
        }
    });
    if let (Some(original), Value::Object(extra)) = (entry["original"].as_object_mut(), extra) {
        original.extend(extra);
    }
    entry
}

#[test]
fn a_pinned_original_is_imported_with_its_port_credited_and_installed() {
    let sha = shotgun_sha();
    let checkout = Checkout::new(
        "pinned",
        json!([
            shotgun(&[&sha], json!({})),
            { "id": "weapon_not_pinned", "original": {
                "addon": "Weapon_Not_Pinned", "title": "Not Pinned", "authors": ["Nobody"],
                "version": "1.0.0", "sha256": [] } },
            { "id": "weapon_pulled", "original": {
                "addon": "Weapon_Pulled", "title": "Pulled", "authors": ["Nobody"],
                "version": "1.0.0", "sha256": ["0".repeat(64)], "withdrawn": "Its author asked" } }
        ]),
    );
    let built = checkout.run(&["build"]);
    let log = text(&built);
    assert!(built.status.success(), "{log}");
    assert!(
        log.contains("Left out Weapon_Not_Pinned: no copy pinned yet"),
        "{log}"
    );
    assert!(
        log.contains("Left out Weapon_Pulled: withdrawn (Its author asked)"),
        "{log}"
    );
    let bundle = checkout.root.join("bundle");
    let info = read(&bundle.join("bundle.json"));
    assert_eq!(
        info["addons"],
        json!([{
            "id": "weapon_shotgun", "addon": "Weapon_Shotgun", "title": "Sawn-off Shotgun",
            "authors": ["Someone", "Someone Else"], "version": "1.2.0", "sha256": sha,
            "port": "weapon_shotgun", "enabled": false
        }])
    );
    // Its package names the authors the list credits, and its port applied.
    let package = bundle.join("addons/weapon_shotgun");
    let manifest = read(&package.join("package.json"));
    assert_eq!(manifest["authors"], json!(["Someone", "Someone Else"]));
    assert_eq!(manifest["name"], "Sawn-off Shotgun");
    assert_eq!(manifest["version"], "1.2.0");
    assert!(
        manifest["provenance"]["bundled"]
            .as_str()
            .unwrap()
            .contains("issue")
    );
    let report = read(&package.join("import-report.json"));
    assert!(
        report["ports"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["port"] == "weapon_shotgun" && p["applied"] == true),
        "{}",
        report["ports"]
    );
    let credits = std::fs::read_to_string(bundle.join("CREDITS.md")).unwrap();
    assert!(
        credits.contains("**Sawn-off Shotgun** by Someone, Someone Else")
            && !credits.contains("Not Pinned")
            && !credits.contains("Pulled"),
        "{credits}"
    );
    assert!(checkout.root.join("bundle.zip").is_file());

    // The packagers' view: our own (none here) and the bundled original.
    let sources = checkout.run(&["sources", "--bundle", bundle.to_str().unwrap()]);
    assert!(sources.status.success(), "{}", text(&sources));
    let sources: Value = serde_json::from_slice(&sources.stdout).unwrap();
    assert_eq!(sources["addons"][0]["id"], "weapon_shotgun");
    assert_eq!(sources["addons"].as_array().unwrap().len(), 1);
    assert!(sources["credits"].as_str().unwrap().ends_with("CREDITS.md"));

    // A checkout's content gets it where the game looks, beside the base game.
    let content = checkout.root.join("content");
    std::fs::create_dir_all(&content).unwrap();
    let installed = checkout.run(&[
        "install",
        "--bundle",
        bundle.to_str().unwrap(),
        "--content-root",
        content.to_str().unwrap(),
    ]);
    assert!(installed.status.success(), "{}", text(&installed));
    assert_eq!(
        read(&content.join("addons/weapon_shotgun/package.json"))["authors"],
        json!(["Someone", "Someone Else"])
    );

    // A release carrying it installed but off, with its credits, verifies;
    // one with the credits missing does not.
    std::fs::write(
        content.join("packages.json"),
        r#"{ "schema_version": 1, "packages": [] }"#,
    )
    .unwrap();
    let credits_path = bundle.join("CREDITS.md");
    let verify = |credits: &Path| {
        checkout.run(&[
            "verify-release",
            content.to_str().unwrap(),
            "--credits",
            credits.to_str().unwrap(),
        ])
    };
    let ok = verify(&credits_path);
    assert!(ok.status.success(), "{}", text(&ok));
    let missing = verify(&checkout.root.join("none.md"));
    assert!(!missing.status.success());
    assert!(
        text(&missing).contains("does not credit Sawn-off Shotgun"),
        "{}",
        text(&missing)
    );
}

#[test]
fn a_copy_the_list_does_not_pin_is_never_bundled() {
    let checkout = Checkout::new(
        "unpinned-copy",
        json!([shotgun(&[&"a".repeat(64)], json!({}))]),
    );
    let built = checkout.run(&["build"]);
    let log = text(&built);
    assert!(!built.status.success(), "{log}");
    assert!(
        log.contains("No pinned copy of Weapon_Shotgun found"),
        "{log}"
    );
    assert!(
        log.contains(&shotgun_sha()),
        "the copy it found is named: {log}"
    );
    assert!(!checkout.root.join("bundle/addons").exists());
}

#[test]
fn find_names_each_copy_its_hash_and_whether_its_port_applies() {
    let checkout = Checkout::new("find", json!([shotgun(&[], json!({}))]));
    let found = checkout.run(&["find"]);
    let log = text(&found);
    assert!(found.status.success(), "{log}");
    assert!(
        log.contains(&format!("sha256 {} (NOT pinned)", shotgun_sha())),
        "{log}"
    );
    assert!(
        log.contains("port weapon_shotgun (verified): applied"),
        "{log}"
    );
}

#[test]
fn originals_are_imported_against_the_generated_game_content() {
    let checkout = Checkout::new("no-content", json!([shotgun(&[&shotgun_sha()], json!({}))]));
    std::fs::remove_dir_all(checkout.root.join("game")).unwrap();
    let built = checkout.run(&["build"]);
    let log = text(&built);
    assert!(!built.status.success(), "{log}");
    assert!(log.contains("No generated game content at"), "{log}");
    assert!(!checkout.root.join("bundle/addons").exists());
}
