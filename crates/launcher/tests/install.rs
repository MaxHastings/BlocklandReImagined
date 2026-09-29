//! Installing, reusing and upgrading the game a standalone exe carries.
use bri_launcher::{CLIENT, GAME_DIR, MARKER, assemble, find_payload, install};
use serde_json::json;
use std::{fs, io::Write, path::Path};

fn release_zip(path: &Path, version: &str, extra: &[(&str, &str)]) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    let top = format!("BlocklandReImagined-{version}-windows");
    let packages = json!({ "schema_version": 1, "packages": [
        { "id": "base-bricks", "version": "1.0.0", "side": "shared", "dir": "bricks", "role": "brick_catalog" },
        { "id": "duplicator", "version": "1.0.0", "side": "shared", "dir": "addons/duplicator" },
    ]});
    let mut files = vec![
        (CLIENT.to_string(), format!("client {version}")),
        ("content/packages.json".into(), packages.to_string()),
        (
            "content/bricks/catalog.json".into(),
            format!("bricks {version}"),
        ),
        ("content/addons/duplicator/package.json".into(), "{}".into()),
    ];
    files.extend(extra.iter().map(|(p, c)| (p.to_string(), c.to_string())));
    zip.add_directory(format!("{top}/"), options).unwrap();
    for (name, body) in files {
        zip.start_file(format!("{top}/{name}"), options).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

fn standalone(dir: &Path, version: &str, extra: &[(&str, &str)]) -> std::path::PathBuf {
    let stub = dir.join("stub.exe");
    fs::write(&stub, b"MZ not a real program").unwrap();
    let zip = dir.join(format!("{version}.zip"));
    release_zip(&zip, version, extra);
    let exe = dir.join(format!("{version}.exe"));
    assemble(&stub, &zip, &exe).unwrap();
    exe
}

#[test]
fn first_run_unpacks_then_reuses_the_install() {
    let temp = tempfile::tempdir().unwrap();
    let exe = standalone(temp.path(), "a1", &[("content/old-only.bin", "x")]);
    let root = temp.path().join("user");
    let payload = find_payload(&exe).unwrap();
    let game = install(&payload, &root).unwrap();
    assert_eq!(game, root.join(GAME_DIR));
    assert_eq!(fs::read_to_string(game.join(CLIENT)).unwrap(), "client a1");
    assert!(game.join(MARKER).is_file());
    // A second start leaves the install alone.
    fs::write(game.join("content/bricks/catalog.json"), "touched").unwrap();
    install(&payload, &root).unwrap();
    assert_eq!(
        fs::read_to_string(game.join("content/bricks/catalog.json")).unwrap(),
        "touched"
    );
    assert!(!root.join("Game.lock").exists());
}

#[test]
fn an_upgrade_keeps_what_the_player_added() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("user");
    let old = standalone(temp.path(), "a1", &[("content/old-only.bin", "x")]);
    let game = install(&find_payload(&old).unwrap(), &root).unwrap();
    // The player drops an old add-on, imports one, turns it on and turns the
    // Duplicator off.
    fs::create_dir_all(game.join("content/Add-Ons")).unwrap();
    fs::write(game.join("content/Add-Ons/Weapon_Gun.zip"), "PK").unwrap();
    fs::create_dir_all(game.join("content/addons/weapon_gun")).unwrap();
    fs::write(game.join("content/addons/weapon_gun/package.json"), "{}").unwrap();
    let mut list: serde_json::Value =
        serde_json::from_slice(&fs::read(game.join("content/packages.json")).unwrap()).unwrap();
    let packages = list["packages"].as_array_mut().unwrap();
    let duplicator = packages.pop().unwrap();
    packages.push(json!({ "id": "weapon_gun", "version": "1.0.0", "side": "shared", "dir": "addons/weapon_gun" }));
    fs::write(game.join("content/packages.json"), list.to_string()).unwrap();
    fs::write(
        game.join("content/packages-disabled.json"),
        json!({ "schema_version": 1, "packages": [duplicator] }).to_string(),
    )
    .unwrap();

    let new = standalone(temp.path(), "a2", &[]);
    let game = install(&find_payload(&new).unwrap(), &root).unwrap();
    assert_eq!(fs::read_to_string(game.join(CLIENT)).unwrap(), "client a2");
    assert_eq!(
        fs::read_to_string(game.join("content/bricks/catalog.json")).unwrap(),
        "bricks a2"
    );
    assert!(
        !game.join("content/old-only.bin").exists(),
        "files only the old version shipped go"
    );
    assert!(game.join("content/Add-Ons/Weapon_Gun.zip").is_file());
    assert!(
        game.join("content/addons/weapon_gun/package.json")
            .is_file()
    );
    let list: serde_json::Value =
        serde_json::from_slice(&fs::read(game.join("content/packages.json")).unwrap()).unwrap();
    let ids: Vec<_> = list["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["base-bricks", "weapon_gun"]);
    let leftovers: Vec<_> = fs::read_dir(&root)
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(leftovers, [std::ffi::OsString::from(GAME_DIR)]);
}

#[test]
fn a_damaged_or_missing_payload_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let plain = temp.path().join("plain.exe");
    fs::write(
        &plain,
        b"MZ no payload here at all, just a program with some bytes",
    )
    .unwrap();
    assert!(find_payload(&plain).is_err());
    let exe = standalone(temp.path(), "a1", &[]);
    let mut bytes = fs::read(&exe).unwrap();
    bytes[40] ^= 0xff;
    fs::write(&exe, bytes).unwrap();
    let error = install(&find_payload(&exe).unwrap(), &temp.path().join("user")).unwrap_err();
    assert!(format!("{error:#}").contains("damaged"), "{error:#}");
    assert!(!temp.path().join("user").join(GAME_DIR).exists());
}
