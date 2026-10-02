//! Server-wide Add-On settings (`scope: "server"`): one value for the whole
//! server that only the host changes, in the Admin menu's Add-On Settings
//! (`HostConfigure`); rules read it with `server_setting(key)` or, by the
//! v20 global it stands for, with `pref(global)` from any Add-On; and one
//! marked `restart` keeps the value it had when the server started or
//! loaded a map. Two made-up packages.
use bri_admin::{Action, Request, ServerSettings};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package::setting::SettingValue as V;
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    session::{Command, PackageCommand, Session},
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};

struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `prefs` declares the settings; `reader`, which does not depend on it,
/// reads them by their globals.
fn catalog(root: &Root) -> Arc<Catalog> {
    let package = |id: &str, behaviour: Value, script: &str| {
        let dir = root.0.join(id);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = json!({
            "schema_version": 1, "id": id, "version": "1.0.0", "api": 1,
            "name": id, "license": "CC0-1.0", "capabilities": [],
            "provides": [
                { "kind": "behaviour", "id": format!("{id}:behaviour/main"), "file": "behaviour.json" },
                { "kind": "script", "id": format!("{id}:script/main"), "file": "main.rhai" }
            ]
        });
        std::fs::write(dir.join("package.json"), manifest.to_string()).unwrap();
        std::fs::write(dir.join("behaviour.json"), behaviour.to_string()).unwrap();
        std::fs::write(dir.join("main.rhai"), script).unwrap();
    };
    package(
        "prefs",
        json!({
            "schema_version": 1,
            "script": "main.rhai",
            "commands": [{ "name": "read", "args": [] }],
            "state": { "global": {
                "speed": { "default": 0, "visible": "everyone" },
                "off": { "default": false, "visible": "everyone" },
                "unset": { "default": false, "visible": "everyone" }
            } },
            "settings": [
                { "key": "speed", "title": "Speed", "scope": "server", "type": "int",
                  "default": 4, "min": 0, "max": 10, "global": "$Pref::Server::Probe::Speed" },
                { "key": "pickups_off", "title": "No Pickups", "scope": "server", "type": "bool",
                  "default": false, "global": "$Pref::Server::Probe::NoPickups", "restart": true }
            ]
        }),
        r#"fn cmd_read(p) { set("speed", server_setting("speed")); set("off", server_setting("pickups_off")); }"#,
    );
    package(
        "reader",
        json!({
            "schema_version": 1,
            "script": "main.rhai",
            "commands": [{ "name": "read", "args": [] }],
            "state": { "global": {
                "speed": { "default": 0, "visible": "everyone" },
                "off": { "default": false, "visible": "everyone" },
                "unset": { "default": false, "visible": "everyone" }
            } }
        }),
        r#"fn cmd_read(p) {
    set("speed", pref("$Pref::Server::Probe::Speed"));
    set("off", pref("$Pref::Server::probe::nopickups"));
    set("unset", pref("$Pref::Server::Nobody::Here") == ());
}"#,
    );
    let entry = |id: &str| PackageEntry {
        id: id.into(),
        version: "1.0.0".into(),
        side: Side::Server,
        dir: id.into(),
        role: None,
    };
    let set = PackageSet {
        schema_version: 1,
        packages: vec![entry("prefs"), entry("reader")],
    };
    Arc::new(Catalog::load(&root.0, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}

struct Game {
    s: Session,
    seq: u64,
    host: OwnerId,
}
/// A server on an empty map with both packages, nobody on it yet.
fn server(root: &Root, settings: ServerSettings) -> Session {
    let mut s = Session::new(
        Simulation::new(
            World::new("Probe".into(), "probe".into(), vec![[1.0; 4]]),
            Definitions::default(),
            vec![],
        )
        .unwrap(),
    );
    s.set_server_settings(settings).unwrap();
    s.install_packages(catalog(root), None).unwrap();
    s
}
impl Game {
    fn new(root: &Root, settings: ServerSettings) -> Self {
        let mut s = server(root, settings);
        let host = s.join("Host".into(), Vec3::ZERO, true).unwrap();
        Self { s, seq: 0, host }
    }
    fn configure(&mut self, values: &[(&str, V)]) -> anyhow::Result<()> {
        let mut settings = self.s.server_settings().clone();
        for (key, value) in values {
            settings.addon_settings.insert((*key).into(), value.clone());
        }
        self.seq += 1;
        self.s
            .command(
                self.host,
                self.seq,
                Command::Admin(Request::new(Action::HostConfigure { settings })),
            )
            .map(drop)
    }
    /// What `package`'s rules read: (speed, pickups off).
    fn read(&mut self, package: &str) -> (Value, Value) {
        self.seq += 1;
        self.s
            .command(
                self.host,
                self.seq,
                Command::Package(PackageCommand {
                    package: package.into(),
                    command: "read".into(),
                    args: vec![],
                }),
            )
            .unwrap();
        let state = self.s.package_state();
        let global = &state.packages[package].global;
        (global["speed"].clone(), global["off"].clone())
    }
}

#[test]
fn the_host_sets_server_settings_rules_read_by_key_or_global() {
    let root =
        Root(std::env::temp_dir().join(format!("bri-server-settings-{}-a", std::process::id())));
    let mut g = Game::new(&root, ServerSettings::default());
    // Players see what each is, the restart one marked.
    let listed = g.s.addon_settings();
    let off = listed.iter().find(|s| s.def.key == "pickups_off").unwrap();
    assert!(off.def.restart);
    assert_eq!(g.read("prefs"), (json!(4), json!(false)), "the defaults");
    assert_eq!(
        g.read("reader"),
        (json!(4), json!(false)),
        "by global, any case"
    );
    assert_eq!(
        g.s.package_state().packages["reader"].global["unset"],
        json!(true)
    );

    assert!(
        g.configure(&[("prefs:speed", V::Int(11))]).is_err(),
        "past its 10"
    );
    assert!(
        g.configure(&[("prefs:speed", V::Bool(true))]).is_err(),
        "a number"
    );
    g.configure(&[("prefs:speed", V::Int(7))]).unwrap();
    assert_eq!(g.read("reader").0, json!(7), "at once");
}

#[test]
fn a_restart_setting_waits_for_the_next_start_or_map() {
    let root =
        Root(std::env::temp_dir().join(format!("bri-server-settings-{}-b", std::process::id())));
    let mut g = Game::new(&root, ServerSettings::default());
    g.configure(&[
        ("prefs:pickups_off", V::Bool(true)),
        ("prefs:speed", V::Int(2)),
    ])
    .unwrap();
    assert_eq!(
        g.read("prefs"),
        (json!(2), json!(false)),
        "kept as it started"
    );
    assert_eq!(g.read("reader").1, json!(false));
    assert_eq!(
        g.s.server_settings().addon_settings["prefs:pickups_off"],
        V::Bool(true),
        "the host's choice is kept for the next start"
    );

    // The server starts again with the host's settings.
    let settings = g.s.server_settings().clone();
    let mut next = Game::new(&root, settings.clone());
    assert_eq!(next.read("prefs"), (json!(2), json!(true)));
    assert_eq!(next.read("reader").1, json!(true));

    // A new map takes over the old one's server and players: a start too.
    let mut map = server(&root, ServerSettings::default());
    map.adopt(g.s, g.host).unwrap();
    let mut map = Game {
        s: map,
        seq: g.seq,
        host: g.host,
    };
    assert_eq!(
        map.read("prefs"),
        (json!(2), json!(true)),
        "the map load took the host's"
    );
}
