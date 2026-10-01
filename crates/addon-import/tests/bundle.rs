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
        copy_port_entries(&repo(), &root);
        std::fs::create_dir_all(root.join("crates/package")).unwrap();
        std::fs::copy(
            repo().join("crates/package/base-packages.json"),
            root.join("crates/package/base-packages.json"),
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
    fixture_sha("Weapon_Shotgun")
}

/// The hash Import Add-On records for the stand-in `fixture`.
fn fixture_sha(fixture: &str) -> String {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = scratch(&format!("sha-{n}")).join("import");
    let report = bri_addon_import::import(&bri_addon_import::Options {
        input: fixtures().join(fixture),
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
            "port": "weapon_shotgun", "companions": [], "enabled": false
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

    // The Mac and Linux builds take the same originals and credits back out
    // of the Windows release: an unpacked one holding them gives a bundle
    // the packagers accept, credits and all.
    let release = checkout
        .root
        .join("release/BlocklandReImagined-test-windows");
    copy_dir(&content.join("addons"), &release.join("content/addons"));
    std::fs::copy(&credits_path, release.join("CREDITS.md")).unwrap();
    let taken = checkout.root.join("taken");
    let out = checkout.run(&[
        "from-release",
        release.to_str().unwrap(),
        "--bundle",
        taken.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(
        std::fs::read_to_string(taken.join("CREDITS.md")).unwrap(),
        credits
    );
    assert_eq!(
        read(&taken.join("addons/weapon_shotgun/package.json")),
        read(&package.join("package.json"))
    );
    let sources = checkout.run(&["sources", "--bundle", taken.to_str().unwrap()]);
    assert!(sources.status.success(), "{}", text(&sources));
    // A release without its credits is refused.
    std::fs::remove_file(release.join("CREDITS.md")).unwrap();
    let refused = checkout.run(&[
        "from-release",
        release.to_str().unwrap(),
        "--bundle",
        taken.to_str().unwrap(),
    ]);
    assert!(!refused.status.success());
    assert!(
        text(&refused).contains("no CREDITS.md"),
        "{}",
        text(&refused)
    );
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &to.join(entry.file_name()));
        } else {
            std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        }
    }
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

/// An original whose port has host rules ships them beside it, laid out as
/// Import leaves them for a player (`addons/<id>` naming its companion
/// `addons/<id>-rules`), through every step a release takes: the bundle,
/// the packagers' sources, a checkout's install, the release check and the
/// Mac and Linux builds' copy. Turned on, the game turns its rules on after
/// it and loads both. Run on the stand-in Player Throwing (CC0), whose
/// listed port writes host rules.
#[test]
fn an_originals_host_rules_ship_beside_it_and_load() {
    const ID: &str = "script_playerthrowing";
    const RULES: &str = "script_playerthrowing-rules";
    let checkout = Checkout::new(
        "rules",
        json!([{ "id": ID, "enabled": true, "original": {
            "addon": "Script_PlayerThrowing", "title": "Player Throwing", "authors": ["Someone"],
            "version": "1.0.0", "sha256": [fixture_sha("Script_PlayerThrowing")] } }]),
    );
    let built = checkout.run(&["build"]);
    let log = text(&built);
    assert!(built.status.success(), "{log}");
    assert!(
        log.contains(&format!("and its host rules {RULES}")),
        "{log}"
    );
    let bundle = checkout.root.join("bundle");
    assert_eq!(
        read(&bundle.join("bundle.json"))["addons"][0]["companions"],
        json!([RULES])
    );
    assert_eq!(
        read(&bundle.join("addons").join(ID).join("package.json"))["companions"],
        json!([RULES])
    );
    assert_eq!(
        read(&bundle.join("addons").join(RULES).join("package.json"))["id"],
        RULES
    );
    // Nothing but the two Add-Ons, and the zip carries both.
    let mut folders: Vec<String> = std::fs::read_dir(bundle.join("addons"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    folders.sort();
    assert_eq!(folders, [ID, RULES]);
    let zip = zip::ZipArchive::new(std::fs::File::open(checkout.root.join("bundle.zip")).unwrap())
        .unwrap();
    assert!(
        zip.file_names()
            .any(|n| n == format!("addons/{RULES}/package.json"))
    );
    assert!(
        !zip.file_names()
            .any(|n| n.split('/').any(|p| p.starts_with('.')))
    );

    // The packagers ship the rules right after it, on with it.
    let sources = checkout.run(&["sources", "--bundle", bundle.to_str().unwrap()]);
    assert!(sources.status.success(), "{}", text(&sources));
    let sources: Value = serde_json::from_slice(&sources.stdout).unwrap();
    let shipped: Vec<(&str, bool)> = sources["addons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| (a["id"].as_str().unwrap(), a["enabled"].as_bool().unwrap()))
        .collect();
    assert_eq!(shipped, [(ID, true), (RULES, true)]);
    assert!(
        sources["addons"][1]["path"]
            .as_str()
            .unwrap()
            .ends_with(RULES)
    );

    // A checkout's install puts both where the game looks; turning the
    // original on in the game turns its rules on after it, and both load.
    let content = checkout.root.join("content");
    std::fs::create_dir_all(&content).unwrap();
    std::fs::write(
        content.join("packages.json"),
        r#"{ "schema_version": 1, "packages": [] }"#,
    )
    .unwrap();
    let installed = checkout.run(&[
        "install",
        "--bundle",
        bundle.to_str().unwrap(),
        "--content-root",
        content.to_str().unwrap(),
    ]);
    assert!(installed.status.success(), "{}", text(&installed));
    use bri_package::library::Library;
    let mut library = Library::scan(&content).unwrap();
    let plan = library.plan(ID, true);
    assert!(plan.allowed(), "{:?}", plan.refused);
    assert_eq!(plan.also, [RULES]);
    library.apply(&plan).unwrap();
    let set = bri_package::packages::PackageSet::load(&content.join("packages.json")).unwrap();
    let on: Vec<&str> = set.packages.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(on, [ID, RULES]);
    bri_package_runtime::Catalog::load(&content, &set, true).unwrap_or_else(|e| panic!("{e:#?}"));

    // That release verifies; one missing the rules, or not turning them
    // on, does not.
    let credits = bundle.join("CREDITS.md");
    let verify = || {
        checkout.run(&[
            "verify-release",
            content.to_str().unwrap(),
            "--credits",
            credits.to_str().unwrap(),
        ])
    };
    let ok = verify();
    assert!(ok.status.success(), "{}", text(&ok));
    let listed = std::fs::read(content.join("packages.json")).unwrap();
    let mut without = set.clone();
    without.packages.retain(|p| p.id != RULES);
    std::fs::write(
        content.join("packages.json"),
        serde_json::to_vec(&without).unwrap(),
    )
    .unwrap();
    let off = verify();
    assert!(!off.status.success());
    assert!(
        text(&off).contains("its host rules would not run"),
        "{}",
        text(&off)
    );
    std::fs::write(content.join("packages.json"), &listed).unwrap();

    // The Mac and Linux builds take the rules out of the Windows release
    // with the original; a release without them is refused.
    let release = checkout
        .root
        .join("release/BlocklandReImagined-test-windows");
    copy_dir(&content.join("addons"), &release.join("content/addons"));
    std::fs::copy(&credits, release.join("CREDITS.md")).unwrap();
    let taken = checkout.root.join("taken");
    let out = checkout.run(&[
        "from-release",
        release.to_str().unwrap(),
        "--bundle",
        taken.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(
        read(&taken.join("addons").join(RULES).join("package.json")),
        read(&bundle.join("addons").join(RULES).join("package.json"))
    );
    assert_eq!(
        read(&taken.join("bundle.json"))["addons"][0]["companions"],
        json!([RULES])
    );
    std::fs::remove_dir_all(release.join("content/addons").join(RULES)).unwrap();
    let refused = checkout.run(&[
        "from-release",
        release.to_str().unwrap(),
        "--bundle",
        taken.to_str().unwrap(),
    ]);
    assert!(!refused.status.success());
    assert!(
        text(&refused).contains(&format!("names its host rules {RULES}")),
        "{}",
        text(&refused)
    );
}

/// Each port's `entry.json`, as the importer finds them.
fn port_entries(repo: &Path) -> Vec<PathBuf> {
    let mut out: Vec<_> = std::fs::read_dir(repo.join("crates/addon-import/ports"))
        .unwrap()
        .flatten()
        .map(|e| e.path().join("entry.json"))
        .filter(|p| p.is_file())
        .collect();
    out.sort();
    out
}

/// Copy every port's `entry.json` into a checkout at `root`.
fn copy_port_entries(repo: &Path, root: &Path) {
    for entry in port_entries(repo) {
        let to = root.join(entry.strip_prefix(repo).unwrap());
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(&entry, to).unwrap();
    }
}
