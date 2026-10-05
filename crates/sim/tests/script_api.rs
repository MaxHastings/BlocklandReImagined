//! The general script API a weapon (or any) Add-On builds on, played through
//! the authoritative session with a made-up package and weapons pack: live
//! rays, the damage rules, damage with a type, what a player holds, swapping
//! the held image, image ammo, the host's field of view, beams, body
//! animations and an image's light-key command.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    presentation::CueKind,
    session::{
        BrickHand, Command, ControlObject, Notice, ObserverButton, OrbitBody, PackageArg,
        PackageCommand, Session,
    },
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::Arc,
    sync::atomic::{AtomicUsize, Ordering},
};

const SCRIPT: &str = r#"
fn note(key, value) { set(key, value); }
fn cmd_ray(p, x, y, z, dx, dy, dz, ignore) {
    let hit = if ignore { raycast([x, y, z], [dx, dy, dz], 50.0, p) }
              else { raycast([x, y, z], [dx, dy, dz], 50.0) };
    note("hit", if hit == () { "none" } else { `${hit.kind}:${hit.id}` });
    note("distance", if hit == () { -1.0 } else { hit.distance });
    note("region", if hit == () || hit.region == () { "" } else { hit.region });
}
fn cmd_region(p, other, y) {
    let at = player(other);
    let region = if at == () { hit_region(other, 0.0, y, 0.0) }
                 else { hit_region(other, at.x, at.y + y, at.z) };
    note("region", if region == () { "none" } else { region });
}
fn cmd_many_rays(p) {
    for i in 0..65 { raycast([0.0, 5.0, 0.0], [0.0, -1.0, 0.0], 10.0); }
}
fn cmd_can(p, other) {
    note("can", `${can_damage(p, other)}|${can_damage(p, "entity:99")}`);
}
fn cmd_hurt(p, other, kind) {
    if kind == "" { damage(other, 30.0, p); } else { damage(other, 30.0, p, kind); }
}
fn cmd_arm(p) { give_item(p, "probe:weapon/gun", true); }
fn cmd_rifle(p) { give_item(p, "probe:weapon/rifle", true); }
fn cmd_mag(p) {
    let m = player(p).magazine;
    note("mag", if m == () { "none" } else {
        let reserve = if m.reserve == () { "endless" } else { `${m.reserve}` };
        `${m.rounds}|${m.size}|${m.ammo}|${reserve}|${m.reloading}`
    });
}
fn cmd_ammo_box(p, n) { give_ammo(p, "probe", n); }
fn cmd_endless(p) { set_reserve(p, "probe", ()); }
fn cmd_load(p, n) { set_rounds(p, "probe:weapon/rifle", n); }
fn cmd_reload_gun(p) { reload(p); }
fn cmd_boom(p, look) {
    if look == "" { explode(0.0, 1.0, 30.0, 2.0, 0.0, 0.0); }
    else { explode(0.0, 1.0, 30.0, 2.0, 0.0, 0.0, look); }
}
fn cmd_facts(p) {
    let me = player(p);
    note("facts", `${me.slot}|${me.image}|${me.image_state}|${me.mounted}|${me.scale}`);
    note("center", me.cy - me.y);
}
fn cmd_scope(p, image) { if image == "" { mount_image(p, ()); } else { mount_image(p, image); } }
fn cmd_ammo(p, ammo) { set_image_ammo(p, ammo); }
fn cmd_fov(p, fov) { if fov < 0.0 { set_fov(p, ()); } else { set_fov(p, fov); } }
fn cmd_show(p) {
    beam([0.0, 1.0, 0.0], [0.0, 1.0, 20.0], #{ color: [1.0, 0.5, 0.0], width: 0.1, seconds: 0.2, muzzle: p });
    play_thread(p, 3, "activate2");
}
fn cmd_reload(p) { note("reloads", get("reloads") + 1); }
fn cmd_lamp(p, x, radius, on) { set_map_lights([x, 2.0, 0.0], radius, #{ on: on, color: [1.0, 0.5, 0.25], brightness: 2.0 }); }
fn cmd_lamp_reset(p, x, radius) { set_map_lights([x, 2.0, 0.0], radius, #{}); }
fn cmd_env_set(p) {
    set_environment(#{ sun_azimuth: 90.0, fog_color: [0.2, 0.3, 0.4], day_length: 60.0,
        time_of_day: 0.25, sun_flare_size: 2.0 });
}
fn cmd_env_read(p) {
    let e = environment();
    note("env", `${e.sun_azimuth}|${e.day_length}|${e.sun_flare_size}|${"fog_color" in e}|${"sky_color" in e}`);
}
fn cmd_env_unset(p) { set_environment(#{ sun_azimuth: (), day_cycle: false }); }
fn cmd_env_bad(p) { set_environment(#{ sun_elevation: 120.0 }); }
fn cmd_env_reset(p) { reset_environment(); }
fn cmd_speed(p, scale) { set_speed_scale(p, scale); }
fn cmd_again(p) { respawn(p); }
fn cmd_dart(p, x, z, vx, vz) { fire("probe:projectile/dart", x, 1.2, z, vx, 0.0, vz); }
fn on_damage(victim, attacker, amount, info) {
    note("struck", if "dx" in info {
        `${info.x.round().to_int()}|${info.dx.round().to_int()}|${info.dy.round().to_int()}|${info.dz.round().to_int()}`
    } else { "nowhere" });
    // A shield: whatever comes at the victim's face (+x here) is blocked.
    if "dx" in info && info.dx < -0.5 { 0.0 } else { () }
}
fn cmd_hold(p, held) { hold_respawn(p, held); }
fn cmd_watch(p, target) { if target < 0 { watch(p, ()); } else { watch(p, target); } }
fn cmd_orbit(p, target, distance) { orbit_camera(p, target, distance); }
fn cmd_orbit_zoom(p, target, near, far, distance) { orbit_camera(p, target, near, far, distance); }
fn cmd_orbit_back(p) { orbit_camera(p, ()); }
fn cmd_keep(p, image) { if image == "" { mount_image(p, (), 3); } else { mount_image(p, image, 3, #{ paint: 2, keep: true }); } }
fn cmd_orbit_frozen(p, target) { orbit_camera(p, target, 4, 9, 6, "frozen"); }
fn cmd_orbit_dazed(p, target) { orbit_camera(p, target, 4, 9, 6, "dazed"); }
fn on_activate(p) { note("heard", get("heard") + "activate "); true }
fn on_observer(p, button) { note("heard", get("heard") + button + " "); true }
fn cmd_put_away(p) { unmount_image(p); }
fn cmd_frame(p, mine, size) {
    let owner = if mine { p } else { () };
    show_shapes(owner, "frame", [
        #{ min: [0.0, 0.0, 0.0], max: [size, size, size], color: [0.0, 0.0, 0.0, 0.35], inside: [0.0, 0.0, 0.0, 0.6] },
        #{ min: [0.0, size, 0.0], max: [0.1, size + 0.1, 0.1], sides: [[1.0, 0.84, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0]], label: "Frame" },
    ]);
}
fn cmd_unframe(p, mine) { hide_shapes(if mine { p } else { () }, "frame"); }
fn cmd_bot(p, name) {
    let kinds = bot_kinds();
    let first = bot_first_name(kinds[0].id, 0);
    note("kinds", `${bot_kind(kinds[0].id).id} ${kinds[0].first_names > 0 && first != ()} ${bot_limit()}`);
    add_bot(player(p).minigame, #{ kind: kinds[0].id, name: name });
}
fn cmd_bots(p) {
    let out = "";
    for b in bots() { out += `${b.name}:${b.spawner}:${b.minigame == player(p).minigame}:${b.item};`; }
    note("bots", out);
}
fn cmd_bots_with(p, kind) {
    let mine = bot_kinds()[0].id;
    let game = player(p).minigame;
    add_bot(game, #{ kind: mine, name: "Bot One" });
    add_bot(game, #{ kind: kind, name: "Bot Two" });
    add_bot(game, #{ kind: mine, name: "Bot Three" });
}
fn cmd_rest(p, b, on) { rest_bot(b, on); }
fn cmd_give(p, b) { give_item(b, "probe:weapon/gun", false); }
fn cmd_bot_tool(p, b, slot) { if slot < 0 { bot_tool(b, ()); } else { bot_tool(b, slot); } }
fn cmd_unbot(p, b) { remove_bot(b); }
fn cmd_box(p) { message_box(p, "Probe", "A box"); }
fn cmd_keep_game(p, v) { let s = get("per_game"); s[`${player(p).minigame}`] = v; set("per_game", s); }
fn cmd_keep_large_game(p) {
    let values = [];
    for i in 0..30 { values.push("xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"); }
    let s = get("per_game");
    s[`${player(p).minigame}`] = values;
    set("per_game", s);
}
fn on_minigame(event) {
    if event.kind == "loaded" {
        note("heard", get("heard") + "loaded ");
        let values = get("per_game");
        let key = `${event.game}`;
        note("loaded_present", key in values);
        note("loaded_value", if key in values { values[key] } else { () });
    }
}
"#;

fn behaviour() -> Value {
    let command = |name: &str, args: &[&str]| json!({ "name": name, "args": args });
    json!({
        "schema_version": 1,
        "script": "main.rhai",
        "on_activate": true,
        "on_observer": true,
        "on_minigame": true,
        "commands": [
            command("ray", &["float", "float", "float", "float", "float", "float", "bool"]),
            command("many_rays", &[]),
            command("can", &["int"]),
            command("hurt", &["int", "string"]),
            command("arm", &[]),
            command("facts", &[]),
            command("scope", &["string"]),
            command("ammo", &["bool"]),
            command("fov", &["float"]),
            command("show", &[]),
            command("reload", &[]),
            command("lamp", &["float", "float", "bool"]),
            command("lamp_reset", &["float", "float"]),
            command("region", &["int", "float"]),
            command("env_set", &[]),
            command("env_read", &[]),
            command("env_unset", &[]),
            command("env_bad", &[]),
            command("env_reset", &[]),
            command("speed", &["float"]),
            command("rifle", &[]),
            command("mag", &[]),
            command("ammo_box", &["int"]),
            command("endless", &[]),
            command("load", &["int"]),
            command("reload_gun", &[]),
            command("boom", &["string"]),
            command("again", &[]),
            command("dart", &["float", "float", "float", "float"]),
            json!({ "name": "hold", "args": ["bool"], "while_dead": true }),
            json!({ "name": "watch", "args": ["int"], "while_dead": true }),
            command("orbit", &["int", "float"]),
            command("orbit_zoom", &["int", "float", "float", "float"]),
            command("orbit_back", &[]),
            command("orbit_frozen", &["int"]),
            command("keep", &["string"]),
            command("orbit_dazed", &["int"]),
            command("put_away", &[]),
            command("frame", &["bool", "float"]),
            command("unframe", &["bool"]),
            command("bot", &["string"]),
            command("bots", &[]),
            command("bots_with", &["string"]),
            command("rest", &["int", "bool"]),
            command("give", &["int"]),
            command("bot_tool", &["int", "int"]),
            command("unbot", &["int"]),
            command("box", &[]),
            command("keep_game", &["string"]),
            command("keep_large_game", &[]),
        ],
        "on_damage": true,
        "state": { "global": {
            "hit": { "default": "", "visible": "everyone" },
            "distance": { "default": 0.0, "visible": "everyone" },
            "region": { "default": "", "visible": "everyone" },
            "can": { "default": "", "visible": "everyone" },
            "facts": { "default": "", "visible": "everyone" },
            "center": { "default": 0.0, "visible": "everyone" },
            "reloads": { "default": 0, "visible": "everyone" },
            "env": { "default": "", "visible": "everyone" },
            "struck": { "default": "", "visible": "everyone" },
            "mag": { "default": "", "visible": "everyone" },
            "heard": { "default": "", "visible": "everyone" },
            "kinds": { "default": "", "visible": "everyone" },
            "bots": { "default": "", "visible": "everyone" },
            "per_game": { "default": {}, "visible": "everyone", "per_minigame": true },
            "loaded_present": { "default": false, "visible": "everyone" },
            "loaded_value": { "default": null, "visible": "everyone" }
        } }
    })
}

/// A gun whose Ready state goes to Empty without ammo, which takes the
/// light key for its reload command, and a scope image to swap in.
fn weapons() -> bri_weapons::Pack {
    let pack = json!({
        "schema_version": 3,
        "id": "probe",
        "items": {
            "probe:weapon/gun": { "ui_name": "Probe Gun", "image": "probe:image/gun" },
            "probe:weapon/rifle": { "ui_name": "Probe Rifle", "image": "probe:image/rifle" }
        },
        "images": {
            "probe:image/gun": {
                "commands": { "light": "probe:reload" },
                "states": [
                    { "name": "Activate", "ticks": 6, "timeout": 1 },
                    { "name": "Ready", "no_ammo": 2 },
                    { "name": "Empty", "ammo": 1 }
                ]
            },
            "probe:image/scope": { "states": [{ "name": "Scoped" }] },
            "probe:image/rifle": {
                "magazine": { "size": 5, "ammo": "probe", "reload_ticks": 24, "reserve": 10,
                              "max_reserve": 40, "display": "Probe Rounds" },
                "states": [
                    { "name": "Activate", "ticks": 6, "timeout": 1 },
                    { "name": "Ready" }
                ]
            }
        },
        "projectiles": {
            "probe:projectile/dart": {
                "speed": 40.0, "lifetime_ticks": 120, "fade_ticks": 120,
                "damage": 10.0, "damage_type": "ProbeShot", "collide_players": true
            }
        },
        "explosions": {
            "probeblast": {
                "name": "probeBlast", "sound": "probeBoomSound", "shake": null, "shape": "",
                "seconds": 0.5, "play_speed": 1.0, "face_viewer": false,
                "scale": [1.0, 1.0, 1.0], "sizes": []
            }
        },
        "damage_types": {
            "probeshot": {
                "name": "ProbeShot",
                "suicide_message": "%1 shot themselves",
                "murder_message": "%2 shot %1",
                "vehicle_scale": 1.0,
                "direct": true
            }
        }
    });
    bri_weapons::Pack::from_json(&serde_json::to_vec(&pack).unwrap()).unwrap()
}

struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn catalog() -> Arc<Catalog> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = Root(std::env::temp_dir().join(format!(
        "bri-script-api-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let dir = root.0.join("probe");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = json!({
        "schema_version": 1, "id": "probe", "version": "1.0.0", "api": 1,
        "name": "probe", "license": "CC0-1.0",
        "capabilities": ["damage", "effects", "player", "lighting", "environment", "minigame", "bots", "chat"],
        "provides": [
            { "kind": "behaviour", "id": "probe:behaviour/main", "file": "behaviour.json" },
            { "kind": "script", "id": "probe:script/main", "file": "main.rhai" }
        ]
    });
    std::fs::write(dir.join("package.json"), manifest.to_string()).unwrap();
    std::fs::write(dir.join("behaviour.json"), behaviour().to_string()).unwrap();
    std::fs::write(dir.join("main.rhai"), SCRIPT).unwrap();
    // Another Add-On built on the probe's images, to change what a player
    // wears behind the probe's back.
    let rival = root.0.join("rival");
    std::fs::create_dir_all(&rival).unwrap();
    let manifest = json!({
        "schema_version": 1, "id": "rival", "version": "1.0.0", "api": 1,
        "name": "rival", "license": "CC0-1.0",
        "capabilities": ["player", "bots"], "dependencies": { "probe": "^1.0.0" },
        "provides": [
            { "kind": "behaviour", "id": "rival:behaviour/main", "file": "behaviour.json" },
            { "kind": "script", "id": "rival:script/main", "file": "main.rhai" }
        ]
    });
    std::fs::write(rival.join("package.json"), manifest.to_string()).unwrap();
    let behaviour = json!({
        "schema_version": 1,
        "script": "main.rhai",
        "commands": [{ "name": "wear", "args": ["string"] }, { "name": "unbot", "args": ["int"] }]
    });
    std::fs::write(rival.join("behaviour.json"), behaviour.to_string()).unwrap();
    std::fs::write(
        rival.join("main.rhai"),
        r#"fn cmd_wear(p, image) { if image == "" { mount_image(p, (), 3); } else { mount_image(p, image, 3); } }
fn cmd_unbot(p, b) { remove_bot(b); }"#,
    )
    .unwrap();
    let entry = |id: &str| PackageEntry {
        id: id.into(),
        version: "1.0.0".into(),
        side: Side::Server,
        dir: id.into(),
        role: None,
    };
    let set = PackageSet {
        schema_version: 1,
        packages: vec![entry("probe"), entry("rival")],
    };
    Arc::new(Catalog::load(&root.0, &set, true).unwrap_or_else(|e| panic!("{e:#?}")))
}

struct Game {
    s: Session,
    seq: u64,
}
impl Game {
    /// Flat ground (tagged as the map, as a loaded map's colliders are),
    /// with the probe package installed.
    fn new() -> Self {
        Self::with_definitions(Definitions::default())
    }
    fn with_definitions(definitions: Definitions) -> Self {
        let ground = ColliderBuilder::cuboid(100.0, 0.5, 100.0)
            .translation(Vector::new(0.0, -0.5, 0.0))
            .user_data(u128::MAX);
        let mut s = Session::new(
            Simulation::new(
                World::new("Probe".into(), "probe".into(), vec![[1.0; 4]]),
                definitions,
                vec![ground],
            )
            .unwrap(),
        );
        s.set_weapon_pack(weapons()).unwrap();
        s.install_packages(catalog(), None).unwrap();
        Self { s, seq: 0 }
    }
    fn join(&mut self, at: Vec3) -> OwnerId {
        self.s.join("P".into(), at, true).unwrap()
    }
    fn send(&mut self, owner: OwnerId, command: Command) -> anyhow::Result<()> {
        self.seq += 1;
        self.s.command(owner, self.seq, command).map(drop)
    }
    fn run(&mut self, owner: OwnerId, command: &str, args: Vec<PackageArg>) {
        self.send(
            owner,
            Command::Package(PackageCommand {
                package: "probe".into(),
                command: command.into(),
                args,
            }),
        )
        .unwrap_or_else(|e| panic!("{command}: {e:#}"));
    }
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.s.step().unwrap();
        }
    }
    fn value(&self, key: &str) -> Value {
        self.s
            .package_state()
            .packages
            .get("probe")
            .and_then(|ns| ns.global.get(key).cloned())
            .unwrap_or(Value::Null)
    }
    fn text(&self, key: &str) -> String {
        self.value(key).as_str().unwrap_or_default().to_string()
    }
    fn diagnostics(&self) -> Vec<String> {
        self.s
            .package_diagnostics()
            .iter()
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect()
    }
}
fn ray(from: [f32; 3], direction: [f32; 3], ignore: bool) -> Vec<PackageArg> {
    from.into_iter()
        .chain(direction)
        .map(|v| PackageArg::Float(v.into()))
        .chain([PackageArg::Bool(ignore)])
        .collect()
}

#[test]
fn raycast_answers_during_the_call_and_passes_through_the_ignored_player() {
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    let b = g.join(Vec3::new(0.0, 0.05, 10.0));
    g.steps(2);
    g.run(a, "ray", ray([0.0, 1.3, 0.0], [0.0, 0.0, 1.0], true));
    assert_eq!(g.text("hit"), format!("player:{b}"));
    let distance = g.value("distance").as_f64().unwrap();
    assert!((8.0..10.0).contains(&distance), "{distance}");
    // A player hit says where: 1.3 up a 2.65 tall blockhead is the legs.
    assert_eq!(g.text("region"), "legs");
    g.run(a, "ray", ray([0.0, 2.5, 0.0], [0.0, 0.0, 1.0], true));
    assert_eq!(g.text("region"), "head");
    // Without `ignore`, the ray starts inside the caster's own body.
    g.run(a, "ray", ray([0.0, 1.3, 0.0], [0.0, 0.0, 1.0], false));
    assert_eq!(g.text("hit"), format!("player:{a}"));
    // Down onto the map, and up into nothing.
    g.run(a, "ray", ray([30.0, 5.0, 30.0], [0.0, -2.0, 0.0], true));
    assert_eq!(g.text("hit"), "map:");
    assert_eq!(g.text("region"), "");
    assert!((g.value("distance").as_f64().unwrap() - 5.0).abs() < 0.01);
    g.run(a, "ray", ray([30.0, 5.0, 30.0], [0.0, 1.0, 0.0], true));
    assert_eq!(g.text("hit"), "none");
    assert!(g.diagnostics().is_empty(), "{:?}", g.diagnostics());
}

/// `hit_region` names the part of a player's body at a height, by the
/// bands of Torque's `getDamageLocation`: the top 15% head, the next 30%
/// torso, the rest legs; () for someone who is not a living player.
#[test]
fn hit_region_names_the_part_of_the_body_at_a_point() {
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    let b = g.join(Vec3::new(0.0, 0.05, 10.0));
    g.steps(2);
    for (y, region) in [(0.4, "legs"), (1.6, "torso"), (2.4, "head")] {
        g.run(
            a,
            "region",
            vec![PackageArg::Int(b as i64), PackageArg::Float(y)],
        );
        assert_eq!(g.text("region"), region, "{y}");
    }
    g.run(
        a,
        "region",
        vec![PackageArg::Int(999), PackageArg::Float(1.0)],
    );
    assert_eq!(g.text("region"), "none");
    assert!(g.diagnostics().is_empty(), "{:?}", g.diagnostics());
}

#[test]
fn a_call_casts_at_most_its_share_of_rays() {
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    let _ = g.send(
        a,
        Command::Package(PackageCommand {
            package: "probe".into(),
            command: "many_rays".into(),
            args: vec![],
        }),
    );
    assert!(
        g.diagnostics()
            .iter()
            .any(|d| d.contains("more than 64 rays")),
        "{:?}",
        g.diagnostics()
    );
}

#[test]
fn damage_follows_the_rules_scripts_ask_about_and_takes_a_type() {
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    let b = g.join(Vec3::new(0.0, 0.05, 10.0));
    // Players outside minigames are never hurt by weapons, and there is no
    // entity 99.
    g.run(a, "can", vec![PackageArg::Int(b as i64)]);
    assert_eq!(g.text("can"), "false|false");
    // Past spawn protection, typed and untyped damage both land.
    g.steps(301);
    g.run(
        a,
        "hurt",
        vec![
            PackageArg::Int(b as i64),
            PackageArg::String("ProbeShot".into()),
        ],
    );
    g.run(
        a,
        "hurt",
        vec![PackageArg::Int(b as i64), PackageArg::String(String::new())],
    );
    assert!((g.s.vitals()[&b].health - 40.0).abs() < 0.01);
    // An unknown damage type is refused and hurts nobody.
    g.run(
        a,
        "hurt",
        vec![PackageArg::Int(b as i64), PackageArg::String("Nope".into())],
    );
    assert!((g.s.vitals()[&b].health - 40.0).abs() < 0.01);
    assert!(
        g.diagnostics()
            .iter()
            .any(|d| d.contains("No damage type `Nope`")),
        "{:?}",
        g.diagnostics()
    );
}

#[test]
fn scripts_see_and_swap_what_a_player_holds() {
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    g.run(a, "arm", vec![]);
    g.steps(20);
    g.run(a, "facts", vec![]);
    assert_eq!(g.text("facts"), "3|probe:image/gun|Ready|false|1.0");
    let center = g.value("center").as_f64().unwrap();
    assert!(
        (1.2..1.4).contains(&center),
        "half the standing body: {center}"
    );
    // Slot 3: the default hammer, wrench and printer come first.
    // Without ammo the gun's Ready state goes to Empty, and back with it.
    g.run(a, "ammo", vec![PackageArg::Bool(false)]);
    g.steps(4);
    g.run(a, "facts", vec![]);
    assert_eq!(g.text("facts"), "3|probe:image/gun|Empty|false|1.0");
    g.run(a, "ammo", vec![PackageArg::Bool(true)]);
    g.steps(4);
    g.run(a, "facts", vec![]);
    assert_eq!(g.text("facts"), "3|probe:image/gun|Ready|false|1.0");
    // A scope swaps in and keeps the tool slot; `()` puts the gun back.
    g.run(
        a,
        "scope",
        vec![PackageArg::String("probe:image/scope".into())],
    );
    g.steps(1);
    g.run(a, "facts", vec![]);
    assert_eq!(g.text("facts"), "3|probe:image/scope|Scoped|false|1.0");
    g.run(a, "scope", vec![PackageArg::String(String::new())]);
    g.steps(20);
    g.run(a, "facts", vec![]);
    assert_eq!(g.text("facts"), "3|probe:image/gun|Ready|false|1.0");
    // Another package's image is refused.
    g.run(
        a,
        "scope",
        vec![PackageArg::String("other:image/scope".into())],
    );
    g.steps(1);
    g.run(a, "facts", vec![]);
    assert_eq!(g.text("facts"), "3|probe:image/gun|Ready|false|1.0");
    assert!(
        g.diagnostics()
            .iter()
            .any(|d| d.contains("is not an image of `probe`")),
        "{:?}",
        g.diagnostics()
    );
}

#[test]
fn the_held_image_takes_the_light_key_for_its_command() {
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    g.send(a, Command::ToggleLight).unwrap();
    assert_eq!(g.value("reloads"), json!(0));
    assert!(
        g.s.vitals()[&a].light,
        "with nothing in hand the light toggles"
    );
    g.send(a, Command::ToggleLight).unwrap();
    g.run(a, "arm", vec![]);
    g.steps(20);
    g.send(a, Command::ToggleLight).unwrap();
    assert_eq!(g.value("reloads"), json!(1));
    assert!(!g.s.vitals()[&a].light, "the gun's command ran instead");
}

#[test]
fn view_beams_and_animations_reach_players_as_notices_and_cues() {
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    g.s.take_private_notices();
    g.run(a, "fov", vec![PackageArg::Float(30.0)]);
    g.run(a, "fov", vec![PackageArg::Float(-1.0)]);
    let fovs: Vec<Option<f32>> =
        g.s.take_private_notices()
            .into_iter()
            .filter_map(|(owner, n)| match n {
                Notice::Fov(fov) if owner == a => Some(fov),
                _ => None,
            })
            .collect();
    assert_eq!(fovs, [Some(30.0), None]);
    g.s.take_cues();
    g.run(a, "show", vec![]);
    let cues = g.s.take_cues();
    assert!(cues.iter().any(|c| matches!(
        &c.kind,
        CueKind::Beam { to, muzzle, width, .. }
            if *to == [0.0, 1.0, 20.0] && *muzzle == Some(a) && *width == 0.1
    ) && c.position == [0.0, 1.0, 0.0]));
    assert!(cues.iter().any(|c| matches!(
        &c.kind,
        CueKind::WeaponAnimation { actor, thread: 3, sequence, image_hand: None }
            if *actor == a && sequence == "activate2"
    )));
    for cue in &cues {
        cue.validate().unwrap();
    }
}

#[test]
fn scripts_switch_dim_and_recolour_map_lights_for_everyone() {
    use bri_sim::session::MapLightRule;
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    assert!(g.s.map_light_rules().is_empty());
    g.run(
        a,
        "lamp",
        vec![
            PackageArg::Float(4.0),
            PackageArg::Float(3.0),
            PackageArg::Bool(true),
        ],
    );
    g.run(
        a,
        "lamp",
        vec![
            PackageArg::Float(-4.0),
            PackageArg::Float(3.0),
            PackageArg::Bool(false),
        ],
    );
    let rules = g.s.map_light_rules();
    assert_eq!(
        rules,
        [
            MapLightRule {
                position: [4.0, 2.0, 0.0],
                radius: 3.0,
                tint: [2.0, 1.0, 0.5]
            },
            MapLightRule {
                position: [-4.0, 2.0, 0.0],
                radius: 3.0,
                tint: [0.0; 3]
            },
        ]
    );
    // What a light inside, outside or between the spheres takes.
    assert_eq!(
        MapLightRule::tint_at(&rules, Vec3::new(4.0, 3.0, 0.0)),
        Vec3::new(2.0, 1.0, 0.5)
    );
    assert_eq!(
        MapLightRule::tint_at(&rules, Vec3::new(-5.0, 2.0, 0.0)),
        Vec3::ZERO
    );
    assert_eq!(
        MapLightRule::tint_at(&rules, Vec3::new(0.0, 2.0, 0.0)),
        Vec3::ONE
    );
    // The same sphere again replaces its rule (so repeated calls never
    // pile up), and an empty map puts the lights back as the map was lit.
    g.run(
        a,
        "lamp_reset",
        vec![PackageArg::Float(4.0), PackageArg::Float(3.0)],
    );
    let rules = g.s.map_light_rules();
    assert_eq!(rules.len(), 2);
    assert_eq!(
        rules[1],
        MapLightRule {
            position: [4.0, 2.0, 0.0],
            radius: 3.0,
            tint: [1.0; 3]
        }
    );
    assert_eq!(
        MapLightRule::tint_at(&rules, Vec3::new(4.0, 2.0, 0.0)),
        Vec3::ONE
    );
    // A later, wider sphere wins where it overlaps.
    g.run(
        a,
        "lamp",
        vec![
            PackageArg::Float(0.0),
            PackageArg::Float(10.0),
            PackageArg::Bool(false),
        ],
    );
    assert_eq!(
        MapLightRule::tint_at(&g.s.map_light_rules(), Vec3::new(4.0, 2.0, 0.0)),
        Vec3::ZERO
    );
}

#[test]
fn scripts_draw_world_shapes_for_everyone() {
    use bri_package_runtime::ops::WorldShape;
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    let b = g.join(Vec3::new(4.0, 0.05, 0.0));
    g.run(
        a,
        "frame",
        vec![PackageArg::Bool(true), PackageArg::Float(2.0)],
    );
    g.run(
        b,
        "frame",
        vec![PackageArg::Bool(true), PackageArg::Float(1.0)],
    );
    g.run(
        a,
        "frame",
        vec![PackageArg::Bool(false), PackageArg::Float(3.0)],
    );
    let sets = g.s.world_shapes();
    let keys: Vec<_> = sets.keys().cloned().collect();
    assert_eq!(
        keys,
        [
            format!("probe/{a}/frame"),
            format!("probe/{b}/frame"),
            "probe/frame".into()
        ]
    );
    assert_eq!(
        *sets[&format!("probe/{a}/frame")],
        [
            WorldShape {
                min: [0.0; 3],
                max: [2.0; 3],
                color: [0, 0, 0, 89],
                inside: [0, 0, 0, 153],
                sides: None,
                label: String::new(),
            },
            WorldShape {
                min: [0.0, 2.0, 0.0],
                max: [0.1, 2.1, 0.1],
                color: [0; 4],
                inside: [0; 4],
                sides: Some([[255, 214, 0, 255], [0, 0, 255, 255], [255; 4]]),
                label: "Frame".into(),
            },
        ]
    );
    // The same set again changes nothing; another replaces it.
    let revision = g.s.world_shapes_revision();
    g.run(
        a,
        "frame",
        vec![PackageArg::Bool(true), PackageArg::Float(2.0)],
    );
    assert_eq!(g.s.world_shapes_revision(), revision);
    g.run(
        a,
        "frame",
        vec![PackageArg::Bool(true), PackageArg::Float(5.0)],
    );
    assert_ne!(g.s.world_shapes_revision(), revision);
    assert_eq!(
        g.s.world_shapes()[&format!("probe/{a}/frame")][0].max,
        [5.0; 3]
    );
    // A player's own sets go when they leave; hiding takes one away.
    g.s.disconnect(b).unwrap();
    assert!(!g.s.world_shapes().contains_key(&format!("probe/{b}/frame")));
    g.run(a, "unframe", vec![PackageArg::Bool(true)]);
    assert_eq!(
        g.s.world_shapes().keys().collect::<Vec<_>>(),
        ["probe/frame"]
    );
    // A box past the longest side is refused.
    let refused = g.send(
        a,
        Command::Package(PackageCommand {
            package: "probe".into(),
            command: "frame".into(),
            args: vec![PackageArg::Bool(false), PackageArg::Float(2000.0)],
        }),
    );
    assert!(format!("{:#}", refused.unwrap_err()).contains("outside the operation's limits"));
    assert_eq!(g.s.world_shapes()["probe/frame"][0].max, [3.0; 3]);
}

#[test]
fn scripts_change_the_environment_for_everyone() {
    use bri_content::atmosphere::SunFlare;
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    assert!(g.s.environment().is_empty());
    g.steps(3);
    let tick = g.s.simulation().state().tick;
    g.run(a, "env_set", vec![]);
    let e = g.s.environment();
    assert_eq!(e.sun_azimuth, Some(90.0));
    assert_eq!(e.fog_color, Some([0.2, 0.3, 0.4]));
    assert_eq!(
        e.sun_flare,
        Some(SunFlare {
            size: 2.0,
            ..SunFlare::default()
        })
    );
    // The cycle starts at the time of day set, from the tick it was set.
    let cycle = e.day_cycle.unwrap();
    assert_eq!(
        (cycle.length_seconds, cycle.time, cycle.anchor_tick),
        (60.0, 0.25, tick)
    );
    g.run(a, "env_read", vec![]);
    assert_eq!(g.text("env"), "90.0|60.0|2.0|true|false");
    // `()` puts one setting back to the map's; the rest stay.
    g.run(a, "env_unset", vec![]);
    let e = g.s.environment();
    assert_eq!((e.sun_azimuth, e.day_cycle), (None, None));
    assert_eq!(e.fog_color, Some([0.2, 0.3, 0.4]));
    // Out-of-range values change nothing.
    let before = g.s.environment();
    let _ = g.send(
        a,
        Command::Package(PackageCommand {
            package: "probe".into(),
            command: "env_bad".into(),
            args: vec![],
        }),
    );
    assert_eq!(g.s.environment(), before);
    g.run(a, "env_reset", vec![]);
    assert!(g.s.environment().is_empty());
}

/// How far `owner` walks forward in two seconds.
fn walk(g: &mut Game, owner: OwnerId) -> f32 {
    let feet = |g: &Game| {
        let p =
            g.s.snapshot()
                .players
                .into_iter()
                .find(|p| p.owner == owner)
                .unwrap();
        Vec3::from(p.feet)
    };
    let start = feet(g);
    for _ in 0..240 {
        g.seq += 1;
        let sequence = g.seq;
        g.s.movement(
            owner,
            sequence,
            bri_sim::player::MoveInput {
                forward: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
        g.s.step().unwrap();
    }
    let moved = feet(g) - start;
    Vec3::new(moved.x, 0.0, moved.z).length()
}

#[test]
fn scripts_slow_a_players_feet_until_they_respawn() {
    let mut g = Game::new();
    let p = g.join(Vec3::new(0.0, 0.1, 0.0));
    g.steps(30);
    let full = walk(&mut g, p);
    assert!(full > 10.0, "a normal walk: {full}");
    g.run(p, "speed", vec![PackageArg::Float(0.5)]);
    g.steps(2);
    let slowed = walk(&mut g, p);
    assert!(
        (slowed / full - 0.5).abs() < 0.08,
        "half speed walks about half as far: {slowed} of {full}"
    );
    g.run(p, "speed", vec![PackageArg::Float(0.0)]);
    g.steps(30);
    assert!(walk(&mut g, p) < 0.5, "held in place");
    let refused = g.send(
        p,
        Command::Package(PackageCommand {
            package: "probe".into(),
            command: "speed".into(),
            args: vec![PackageArg::Float(9.0)],
        }),
    );
    assert!(
        refused.is_err_and(|e| format!("{e:#}").contains("outside the operation's limits")),
        "4 at most"
    );
    g.run(p, "again", vec![]);
    g.steps(240);
    let fresh = walk(&mut g, p);
    assert!(
        (fresh / full - 1.0).abs() < 0.08,
        "a new body walks normally: {fresh} of {full}"
    );
}

#[test]
fn damage_hooks_see_where_a_shot_struck_and_which_way_it_flew() {
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.1, 0.0));
    let b = g.join(Vec3::new(6.0, 0.1, 0.0));
    g.steps(301);
    let dart = |x: f32, vx: f32| {
        [x, 0.0, vx, 0.0]
            .into_iter()
            .map(|v| PackageArg::Float(v.into()))
            .collect::<Vec<_>>()
    };
    // From behind (+x travel): it lands on b's near side, and the hook
    // reads the hit.
    g.run(a, "dart", dart(3.0, 40.0));
    g.steps(12);
    assert_eq!(g.text("struck"), "5|1|0|0", "{:?}", g.diagnostics());
    let health = g.s.vitals()[&b].health;
    assert!((health - 90.0).abs() < 0.01, "{health}");
    // Head on (-x travel): the hook's shield takes it all.
    g.run(a, "dart", dart(9.0, -40.0));
    g.steps(12);
    assert_eq!(g.text("struck"), "7|-1|0|0");
    assert!((g.s.vitals()[&b].health - health).abs() < 0.01);
    // Damage with no shot behind it has no hit to read.
    g.run(
        a,
        "hurt",
        vec![PackageArg::Int(b as i64), PackageArg::String(String::new())],
    );
    assert_eq!(g.text("struck"), "nowhere");
}

fn bottom_prints(g: &mut Game, owner: OwnerId) -> Vec<String> {
    g.s.take_private_notices()
        .into_iter()
        .filter_map(|(o, n)| match n {
            Notice::Bottom { text, .. } if o == owner => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn a_magazine_shows_its_rounds_reloads_on_the_light_key_and_takes_ammo_from_rules() {
    let mut g = Game::new();
    let p = g.join(Vec3::new(0.0, 0.05, 0.0));
    g.steps(3);
    g.s.take_private_notices();
    g.run(p, "rifle", vec![]);
    g.steps(10);
    let prints = bottom_prints(&mut g, p);
    let last = prints.last().expect("drawing it shows the ammo display");
    assert!(
        last.contains("Probe Rounds") && last.contains(">5 ") && last.contains("/ 10"),
        "{last}"
    );
    g.run(p, "mag", vec![]);
    assert_eq!(g.text("mag"), "5|5|probe|10|false");
    g.run(p, "load", vec![PackageArg::Int(2)]);
    g.send(p, Command::ToggleLight).unwrap();
    g.steps(1);
    g.run(p, "mag", vec![]);
    assert_eq!(g.text("mag"), "2|5|probe|10|true", "the light key reloads");
    assert!(
        bottom_prints(&mut g, p)
            .last()
            .unwrap()
            .contains("Reloading")
    );
    g.steps(30);
    g.run(p, "mag", vec![]);
    assert_eq!(g.text("mag"), "5|5|probe|7|false");
    // A rule's ammo box tops up to the most the magazine carries; the
    // light key with a full magazine does nothing.
    g.run(p, "ammo_box", vec![PackageArg::Int(100)]);
    g.run(p, "reload_gun", vec![]);
    g.steps(1);
    g.run(p, "mag", vec![]);
    assert_eq!(g.text("mag"), "5|5|probe|40|false");
    g.run(p, "endless", vec![]);
    g.steps(1);
    g.run(p, "mag", vec![]);
    assert_eq!(g.text("mag"), "5|5|probe|endless|false");
    let refused = g.send(
        p,
        Command::Package(PackageCommand {
            package: "probe".into(),
            command: "ammo_box".into(),
            args: vec![PackageArg::Int(0)],
        }),
    );
    assert!(
        refused.is_err_and(|e| format!("{e:#}").contains("outside the operation's limits")),
        "at least one round"
    );
    // A gun without a magazine clears the display and keeps its own light
    // key command.
    g.s.take_private_notices();
    g.run(p, "arm", vec![]);
    g.steps(10);
    assert_eq!(bottom_prints(&mut g, p), [String::new()]);
    g.run(p, "mag", vec![]);
    assert_eq!(g.text("mag"), "none");
}

#[test]
fn an_add_on_explosion_looks_and_sounds_like_its_own() {
    let mut g = Game::new();
    let p = g.join(Vec3::new(0.0, 0.05, 0.0));
    g.steps(3);
    g.s.take_cues();
    g.run(p, "boom", vec![PackageArg::String("probeBlast".into())]);
    let cues = g.s.take_cues();
    assert!(cues.iter().any(|c| matches!(
        &c.kind,
        CueKind::WeaponEffect { definition, .. } if definition == "probeblast"
    )));
    assert!(cues.iter().any(|c| matches!(
        &c.kind,
        CueKind::WeaponSound { profile } if profile == "probeBoomSound"
    )));
    assert!(
        !cues
            .iter()
            .any(|c| matches!(c.kind, CueKind::Explosion { .. }))
    );
    for cue in &cues {
        cue.validate().unwrap();
    }
    // Without a name it is the engine's own blast; an unknown one is refused.
    g.run(p, "boom", vec![PackageArg::String(String::new())]);
    assert!(
        g.s.take_cues()
            .iter()
            .any(|c| matches!(c.kind, CueKind::Explosion { .. }))
    );
    let _ = g.send(
        p,
        Command::Package(PackageCommand {
            package: "probe".into(),
            command: "boom".into(),
            args: vec![PackageArg::String("nothing".into())],
        }),
    );
    assert!(g.s.take_cues().is_empty());
}

#[test]
fn a_rule_holds_a_respawn_until_reset_and_points_a_camera_elsewhere() {
    use bri_minigames::Settings;
    use bri_sim::session::{ControlObject, MiniGameRequest};
    let mut g = Game::new();
    let a = g.join(Vec3::new(-3.0, 0.05, 0.0));
    let b = g.join(Vec3::new(3.0, 0.05, 0.0));
    // The probe weapons pack replaces the stock items.
    let settings = Settings {
        loadout: Default::default(),
        ..Settings::default()
    };
    g.send(
        a,
        Command::MiniGame(MiniGameRequest::Create { color: 0, settings }),
    )
    .unwrap();
    let game = g.s.minigame_views()[0].id;
    g.send(b, Command::MiniGame(MiniGameRequest::Join { game }))
        .unwrap();

    // Out of lives: held, so clicking does nothing and the client is told.
    g.send(b, Command::Suicide).unwrap();
    g.run(b, "hold", vec![PackageArg::Bool(true)]);
    g.steps(600);
    assert!(g.s.vitals()[&b].respawn_held);
    let refused = g.send(b, Command::Respawn).unwrap_err();
    assert!(
        format!("{refused:#}").contains("RespawnHeld"),
        "{refused:#}"
    );
    // Let go: the click works again.
    g.run(b, "hold", vec![PackageArg::Bool(false)]);
    assert!(!g.s.vitals()[&b].respawn_held);
    g.send(b, Command::Respawn).unwrap();
    assert!(g.s.vitals()[&b].alive);

    // A reset frees a held player.
    g.send(b, Command::Suicide).unwrap();
    g.run(b, "hold", vec![PackageArg::Bool(true)]);
    g.send(a, Command::MiniGame(MiniGameRequest::Reset))
        .unwrap();
    assert!(g.s.vitals()[&b].alive && !g.s.vitals()[&b].respawn_held);

    // Watching another player: the frozen orbit camera around them at the
    // corpse camera's 8 units. The body neither fires nor uses tools until
    // the rule hands control back.
    let watching = |target| ControlObject::Orbit {
        target,
        min: 8,
        max: 8,
        distance: 8,
        body: OrbitBody::Frozen,
    };
    g.run(a, "watch", vec![PackageArg::Int(b as i64)]);
    assert_eq!(g.s.control(a), Some(watching(b)));
    let refused = g
        .send(a, Command::WeaponTrigger { down: true })
        .unwrap_err();
    assert!(format!("{refused:#}").contains("watching"), "{refused:#}");
    assert!(g.send(a, Command::EquipTool { slot: Some(0) }).is_err());
    g.send(a, Command::WeaponTrigger { down: false }).unwrap();
    // Watching yourself orbits your own body.
    g.run(a, "watch", vec![PackageArg::Int(a as i64)]);
    assert_eq!(g.s.control(a), Some(watching(a)));
    g.run(a, "watch", vec![PackageArg::Int(-1)]);
    assert_eq!(g.s.control(a), Some(ControlObject::Player));
    // A respawn hands control back too.
    g.run(b, "watch", vec![PackageArg::Int(a as i64)]);
    g.send(b, Command::Suicide).unwrap();
    g.steps(600);
    g.send(a, Command::MiniGame(MiniGameRequest::Reset))
        .unwrap();
    assert_eq!(g.s.control(b), Some(ControlObject::Player));
    assert!(g.diagnostics().is_empty(), "{:?}", g.diagnostics());
}

/// `orbit_camera`: an Add-On hands a player an orbit camera around another
/// (`setOrbitMode`, `setControlObject(camera)`) at its own distance. The
/// player cannot click their way out of it, the Add-On ends it, and so does
/// the target leaving. An admin's camera is not taken over.
#[test]
fn an_add_on_orbits_a_players_camera_around_another() {
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    let b = g.join(Vec3::new(3.0, 0.05, 0.0));
    let orbit = |target: OwnerId, distance: f64| {
        vec![PackageArg::Int(target as i64), PackageArg::Float(distance)]
    };
    g.run(a, "orbit", orbit(b, 6.0));
    assert_eq!(
        g.s.vitals()[&a].control,
        ControlObject::Orbit {
            target: b,
            min: 6,
            max: 6,
            distance: 6,
            body: OrbitBody::Acts,
        }
    );
    // With a zoom range (`setOrbitMode(%b, 0, 5, 10, 5, 0)`).
    let zoom = |near: f64, far: f64, distance: f64| {
        vec![
            PackageArg::Int(b as i64),
            PackageArg::Float(near),
            PackageArg::Float(far),
            PackageArg::Float(distance),
        ]
    };
    g.run(a, "orbit_zoom", zoom(5.0, 10.0, 5.0));
    assert_eq!(
        g.s.vitals()[&a].control,
        ControlObject::Orbit {
            target: b,
            min: 5,
            max: 10,
            distance: 5,
            body: OrbitBody::Acts,
        }
    );
    // Starting outside its range: refused, the orbit stays as it was.
    assert!(
        g.send(
            a,
            Command::Package(PackageCommand {
                package: "probe".into(),
                command: "orbit_zoom".into(),
                args: zoom(5.0, 10.0, 12.0),
            }),
        )
        .is_err()
    );
    assert!(matches!(
        g.s.vitals()[&a].control,
        ControlObject::Orbit { max: 10, .. }
    ));
    assert!(
        g.send(a, Command::ControlPlayer).is_err(),
        "the Add-On's to end"
    );
    // A rule's `watch(p, ())` ends only the frozen kind.
    g.run(a, "watch", vec![PackageArg::Int(-1)]);
    assert!(matches!(
        g.s.vitals()[&a].control,
        ControlObject::Orbit { max: 10, .. }
    ));
    g.run(a, "orbit_back", vec![]);
    assert_eq!(g.s.vitals()[&a].control, ControlObject::Player);
    // Out of range or around oneself: refused, nothing changes.
    for (target, distance) in [(b, 40.0), (a, 6.0)] {
        let _ = g.send(
            a,
            Command::Package(PackageCommand {
                package: "probe".into(),
                command: "orbit".into(),
                args: orbit(target, distance),
            }),
        );
    }
    assert_eq!(g.s.vitals()[&a].control, ControlObject::Player);
    // The target leaving gives the body back.
    g.run(a, "orbit", orbit(b, 6.0));
    g.s.disconnect(b).unwrap();
    assert_eq!(g.s.vitals()[&a].control, ControlObject::Player);
    // An admin's free camera stays theirs.
    let c = g.join(Vec3::new(-3.0, 0.05, 0.0));
    g.send(
        a,
        Command::Admin(bri_admin::Request::new(
            bri_admin::Action::DropCameraAtPlayer,
        )),
    )
    .unwrap();
    assert_eq!(g.s.vitals()[&a].control, ControlObject::Camera);
    g.run(a, "orbit", orbit(c, 6.0));
    assert_eq!(g.s.vitals()[&a].control, ControlObject::Camera);
}

/// `unmount_image` empties the hand: bricks in hand are put away too, and
/// the client is told, since it owns the brick choice.
#[test]
fn unmount_image_puts_away_bricks_in_hand() {
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    let hand = BrickHand {
        stocked: true,
        equipped: true,
        ghost: false,
    };
    g.send(a, Command::BrickHand(hand)).unwrap();
    g.s.take_private_notices();
    g.run(a, "put_away", vec![]);
    let notices = g.s.take_private_notices();
    assert!(
        notices
            .iter()
            .any(|(o, n)| *o == a && matches!(n, Notice::PutAway)),
        "{notices:?}"
    );
}

/// One orbit camera, two bodies. Throwing's held player keeps acting: the
/// body takes no moves, but the click is still their empty-hand trigger
/// for Add-Ons (`on_activate`), and no spectator's keys. A rule's frozen
/// orbit (`watch`, v20's `setControlObject(camera)`) is the other way
/// round: the body takes no actions, and every key, the click too, goes to
/// the rules (`on_observer`), never back to the body.
#[test]
fn an_orbit_camera_either_lets_the_body_act_or_freezes_it() {
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    let b = g.join(Vec3::new(3.0, 0.05, 0.0));
    // Acting: the click reaches `on_activate`; spectator keys are refused.
    g.run(
        a,
        "orbit",
        vec![PackageArg::Int(b as i64), PackageArg::Float(6.0)],
    );
    g.send(a, Command::Activate).unwrap();
    assert_eq!(g.text("heard"), "activate ");
    assert!(
        g.send(a, Command::ObserverButton(ObserverButton::Fire))
            .is_err()
    );
    assert!(g.send(a, Command::ControlPlayer).is_err());
    g.run(a, "orbit_back", vec![]);
    assert_eq!(g.s.control(a), Some(ControlObject::Player));

    // Frozen, as `orbit_camera`'s option: keys reach `on_observer`; the
    // click acts no more and does not hand the body back.
    g.run(a, "orbit_frozen", vec![PackageArg::Int(b as i64)]);
    assert_eq!(
        g.s.control(a),
        Some(ControlObject::Orbit {
            target: b,
            min: 4,
            max: 9,
            distance: 6,
            body: OrbitBody::Frozen,
        })
    );
    assert!(g.send(a, Command::Activate).is_err());
    for button in [ObserverButton::Fire, ObserverButton::Jet] {
        g.send(a, Command::ObserverButton(button)).unwrap();
    }
    assert_eq!(g.text("heard"), "activate fire jet ", "not activated again");
    assert!(
        g.send(a, Command::ControlPlayer).is_err(),
        "the rules' to end"
    );
    // An acting orbit's `orbit_camera(p, ())` leaves it alone, and an
    // acting orbit is not laid over it.
    g.run(a, "orbit_back", vec![]);
    let _ = g.send(
        a,
        Command::Package(PackageCommand {
            package: "probe".into(),
            command: "orbit".into(),
            args: vec![PackageArg::Int(b as i64), PackageArg::Float(6.0)],
        }),
    );
    assert!(matches!(
        g.s.control(a),
        Some(ControlObject::Orbit {
            body: OrbitBody::Frozen,
            ..
        })
    ));
    g.run(a, "watch", vec![PackageArg::Int(-1)]);
    assert_eq!(g.s.control(a), Some(ControlObject::Player));
    // A body is "acts" or "frozen".
    assert!(
        g.send(
            a,
            Command::Package(PackageCommand {
                package: "probe".into(),
                command: "orbit_dazed".into(),
                args: vec![PackageArg::Int(b as i64)],
            }),
        )
        .is_err()
    );
    assert_eq!(g.s.control(a), Some(ControlObject::Player));

    // The dead are given only the frozen kind: watching their own body is
    // the corpse camera.
    g.send(a, Command::Suicide).unwrap();
    g.run(a, "watch", vec![PackageArg::Int(b as i64)]);
    assert!(matches!(
        g.s.control(a),
        Some(ControlObject::Orbit {
            target,
            body: OrbitBody::Frozen,
            ..
        }) if target == b
    ));
    g.run(a, "watch", vec![PackageArg::Int(a as i64)]);
    assert_eq!(g.s.control(a), Some(ControlObject::Corpse));
}

/// `mount_image(p, image, slot, #{ keep: true })`: while the image is worn
/// no other Add-On replaces or takes it off (Slayer CTF's
/// `Player::mountImage` and `unMountImage` overrides guarding a carried
/// flag); its own Add-On still may, and once it is off, by death too, the
/// slot is anyone's again.
#[test]
fn a_kept_worn_image_is_only_its_add_ons_to_change() {
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    let worn = |g: &Game| -> Vec<(String, Option<u8>)> {
        g.s.weapon_view()
            .images
            .get(&a)
            .map(|images| images.iter().map(|i| (i.image.clone(), i.paint)).collect())
            .unwrap_or_default()
    };
    let rival = |g: &mut Game, image: &str| {
        g.send(
            a,
            Command::Package(PackageCommand {
                package: "rival".into(),
                command: "wear".into(),
                args: vec![PackageArg::String(image.into())],
            }),
        )
    };
    let gun = "probe:image/gun";
    let scope = "probe:image/scope";
    g.run(a, "keep", vec![PackageArg::String(gun.into())]);
    assert_eq!(worn(&g), [(gun.to_string(), Some(2))]);
    // The rival's tries are refused, each with a diagnostic.
    let refusals = |g: &Game| {
        g.diagnostics()
            .iter()
            .filter(|d| d.contains("keeps"))
            .count()
    };
    rival(&mut g, scope).unwrap();
    rival(&mut g, "").unwrap();
    assert_eq!(worn(&g), [(gun.to_string(), Some(2))], "still worn");
    assert_eq!(refusals(&g), 2, "{:?}", g.diagnostics());
    // Its own Add-On changes it.
    g.run(a, "keep", vec![PackageArg::String(String::new())]);
    assert!(worn(&g).is_empty());
    rival(&mut g, scope).unwrap();
    assert_eq!(worn(&g), [(scope.to_string(), None)]);
    // Kept again, then off with the body: the next one is anyone's.
    g.run(a, "keep", vec![PackageArg::String(gun.into())]);
    g.send(a, Command::Suicide).unwrap();
    g.steps(600);
    g.send(a, Command::Respawn).unwrap();
    rival(&mut g, scope).unwrap();
    assert_eq!(worn(&g), [(scope.to_string(), None)]);
    assert_eq!(refusals(&g), 2, "{:?}", g.diagnostics());
}

#[test]
fn a_mini_games_rules_add_rest_arm_and_take_away_their_own_bots() {
    use bri_minigames::Settings;
    use bri_sim::session::MiniGameRequest;
    let mut g = Game::new();
    g.s.set_bot_kinds(
        bri_sim::bot_kind::BotPack::from_json(include_bytes!(
            "../../../packages/blockhead_bot/assets/bots.json"
        ))
        .unwrap()
        .bots,
    )
    .unwrap();
    // A map's drop point, where it comes in.
    g.s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 8.0)])
        .unwrap();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    let settings = Settings {
        loadout: Default::default(),
        ..Settings::default()
    };
    g.send(
        a,
        Command::MiniGame(MiniGameRequest::Create { color: 0, settings }),
    )
    .unwrap();
    let game = g.s.minigame_views()[0].id;

    // A bot of the kind an Add-On provides joins the game, its spawner
    // the package that added it.
    g.run(a, "bot", vec![PackageArg::String("Bot Probe".into())]);
    g.steps(2);
    assert_eq!(g.text("kinds"), "bot.blockhead true 16");
    let bots: Vec<OwnerId> =
        g.s.vitals()
            .keys()
            .copied()
            .filter(|o| g.s.is_bot(*o))
            .collect();
    assert_eq!(bots.len(), 1, "{:?}", g.diagnostics());
    let bot = bots[0];
    assert_eq!(g.s.vitals()[&bot].minigame, Some(game));
    assert!(g.s.vitals()[&bot].alive);
    g.run(a, "bots", vec![]);
    assert_eq!(g.text("bots"), "Bot Probe:probe:true:;");

    // It roams; rested, it stands still.
    let feet = |g: &Game| -> Vec3 {
        g.s.motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == bot)
            .unwrap()
            .0
            .feet
            .into()
    };
    let start = feet(&g);
    let mut moved = 0.0f32;
    for _ in 0..90 {
        g.steps(10);
        moved = moved.max(feet(&g).distance(start));
    }
    assert!(moved > 0.5, "roams: {moved}");
    let bot_arg = || PackageArg::Int(bot as i64);
    g.run(a, "rest", vec![bot_arg(), PackageArg::Bool(true)]);
    g.steps(30);
    let rested = feet(&g);
    g.steps(900);
    assert!(feet(&g).distance(rested) < 0.05, "{rested} {}", feet(&g));

    // Its tools: put away, then one in its hand (rested, so it does not
    // draw a weapon on the player it sees).
    g.run(a, "give", vec![bot_arg()]);
    g.run(a, "bot_tool", vec![bot_arg(), PackageArg::Int(-1)]);
    g.steps(2);
    g.run(a, "bots", vec![]);
    assert_eq!(g.text("bots"), "Bot Probe:probe:true:;");
    g.run(a, "bot_tool", vec![bot_arg(), PackageArg::Int(0)]);
    g.steps(2);
    g.run(a, "bots", vec![]);
    assert_eq!(g.text("bots"), "Bot Probe:probe:true:probe:weapon/gun;");

    // Another Add-On cannot take it away; its own can.
    g.send(
        a,
        Command::Package(PackageCommand {
            package: "rival".into(),
            command: "unbot".into(),
            args: vec![bot_arg()],
        }),
    )
    .unwrap();
    g.steps(2);
    assert!(g.s.is_bot(bot));
    assert!(
        g.diagnostics()
            .iter()
            .any(|d| d.contains("not one `rival` added"))
    );
    g.run(a, "unbot", vec![bot_arg()]);
    g.steps(2);
    assert!(!g.s.is_bot(bot));
    assert!(!g.s.vitals().contains_key(&bot));

    // The server's bot limit holds.
    for i in 0..20 {
        g.run(a, "bot", vec![PackageArg::String(format!("Bot {i}"))]);
        g.steps(1);
    }
    g.steps(2);
    let count = |g: &Game| g.s.vitals().keys().filter(|o| g.s.is_bot(**o)).count();
    assert_eq!(count(&g), 16);
    assert!(
        g.diagnostics()
            .iter()
            .any(|d| d.contains("limited to 16 bots"))
    );
    // They leave with their game.
    g.send(a, Command::MiniGame(MiniGameRequest::End)).unwrap();
    g.steps(2);
    assert_eq!(count(&g), 0);

    // A message box reaches its player.
    g.s.take_private_notices();
    g.run(a, "box", vec![]);
    g.steps(1);
    assert!(g.s.take_private_notices().iter().any(|(o, n)| *o == a
        && matches!(n, Notice::MessageBox { title, text } if title == "Probe" && text == "A box")));
}

/// A rules bot's name past the limit keeps its first characters: one long
/// name must not throw away everything else the call did (the bot, and
/// the rest of a Preferred Player Count fill with it).
#[test]
fn a_rules_bots_long_name_is_cut_to_the_limit_not_refused() {
    use bri_minigames::Settings;
    use bri_sim::session::MiniGameRequest;
    let mut g = Game::new();
    g.s.set_bot_kinds(
        bri_sim::bot_kind::BotPack::from_json(include_bytes!(
            "../../../packages/blockhead_bot/assets/bots.json"
        ))
        .unwrap()
        .bots,
    )
    .unwrap();
    // A map's drop points, where they come in.
    g.s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 8.0), Vec3::new(4.0, 0.05, 8.0)])
        .unwrap();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    let settings = Settings {
        loadout: Default::default(),
        ..Settings::default()
    };
    g.send(
        a,
        Command::MiniGame(MiniGameRequest::Create { color: 0, settings }),
    )
    .unwrap();
    g.run(
        a,
        "bot",
        vec![PackageArg::String(
            "Bot Bartholomew-Fitzgerald-Smythe".into(),
        )],
    );
    g.steps(2);
    assert_eq!(g.text("kinds"), "bot.blockhead true 16");
    g.run(a, "bots", vec![]);
    assert_eq!(g.text("bots"), "Bot Bartholomew-Fitzger:probe:true:;");
    assert!(g.diagnostics().is_empty(), "{:?}", g.diagnostics());
}

/// One bot of a fill that cannot join (a kind no Add-On provides) leaves
/// the others of the same call joining, and the failure is never memory
/// only: the admins read it in chat, once a minute, and the host takes it
/// for its Add-On health.
#[test]
fn a_bot_that_cannot_join_is_told_and_the_rest_of_the_fill_joins() {
    use bri_minigames::Settings;
    use bri_sim::session::MiniGameRequest;
    let mut g = Game::new();
    g.s.set_bot_kinds(
        bri_sim::bot_kind::BotPack::from_json(include_bytes!(
            "../../../packages/blockhead_bot/assets/bots.json"
        ))
        .unwrap()
        .bots,
    )
    .unwrap();
    // A map's drop points, where they come in.
    g.s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 8.0), Vec3::new(4.0, 0.05, 8.0)])
        .unwrap();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    let settings = Settings {
        loadout: Default::default(),
        ..Settings::default()
    };
    g.send(
        a,
        Command::MiniGame(MiniGameRequest::Create { color: 0, settings }),
    )
    .unwrap();
    g.s.take_private_notices();
    g.s.take_package_problems();
    g.run(
        a,
        "bots_with",
        vec![PackageArg::String("bot.missing".into())],
    );
    g.steps(2);
    g.run(a, "bots", vec![]);
    assert_eq!(
        g.text("bots"),
        "Bot One:probe:true:;Bot Three:probe:true:;",
        "{:?}",
        g.diagnostics()
    );
    let told = |g: &mut Game| {
        g.s.take_private_notices()
            .into_iter()
            .filter(|(o, n)| {
                *o == a
                    && matches!(n, Notice::Chat(t)
                        if t.contains("Add-On probe: No bot kind `bot.missing`"))
            })
            .count()
    };
    assert_eq!(told(&mut g), 1);
    let taken = g.s.take_package_problems();
    assert!(
        taken
            .iter()
            .any(|d| d.code == "op.failed" && d.message.contains("bot.missing")),
        "{taken:?}"
    );
    assert!(g.s.take_package_problems().is_empty(), "taken once");
    // Again within the minute: kept for the host, not told again.
    g.run(
        a,
        "bots_with",
        vec![PackageArg::String("bot.missing".into())],
    );
    g.steps(2);
    assert_eq!(told(&mut g), 0);
    assert!(!g.s.take_package_problems().is_empty());
}

#[test]
fn a_saved_build_brings_back_its_mini_game_and_the_add_on_state_kept_per_game() {
    use bri_minigames::Settings;
    use bri_sim::session::{MiniGameRequest, Reply};
    let mut g = Game::new();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    let b = g.join(Vec3::new(4.0, 0.05, 0.0));
    let settings = Settings {
        title: "Saved".into(),
        points_kill_player: 7,
        loadout: Default::default(),
        ..Settings::default()
    };
    g.send(
        a,
        Command::MiniGame(MiniGameRequest::Create {
            color: 2,
            settings: settings.clone(),
        }),
    )
    .unwrap();
    let game = g.s.minigame_views()[0].id;
    g.run(a, "keep_game", vec![PackageArg::String("path".into())]);
    let save = |g: &mut Game, who: OwnerId| {
        g.seq += 1;
        match g.s.command(
            who,
            g.seq,
            Command::SaveBuild {
                events: true,
                ownership: false,
            },
        ) {
            Ok(Reply::Saved(build)) => build,
            other => panic!("{other:?}"),
        }
    };
    // Someone running no mini-game saves none.
    assert_eq!(save(&mut g, b).minigame, None);
    let mut build = save(&mut g, a);
    assert!(build.minigame.is_some());
    // A brick of a kind this test's server lacks: it is kept, not placed.
    build.world.bricks.insert(
        1,
        bri_world::Brick::new(
            bri_world::ContentRef::Resolved("brick/none".into()),
            [0.0; 3],
            0,
        ),
    );
    build.world.next_brick_id = 2;

    // Changed since, then the build loads into the game its loader runs.
    let mut changed = settings.clone();
    changed.title = "Changed".into();
    g.send(
        a,
        Command::MiniGame(MiniGameRequest::Configure { settings: changed }),
    )
    .unwrap();
    g.run(a, "keep_game", vec![PackageArg::String("other".into())]);
    g.steps(5 * 120);
    g.send(
        a,
        Command::LoadBuild {
            build,
            ownership: false,
        },
    )
    .unwrap();
    while g.s.build_loading() {
        g.steps(1);
    }
    g.steps(2);
    let view = g.s.minigame_views();
    assert_eq!(view.len(), 1);
    assert_eq!(view[0].id, game);
    assert_eq!(view[0].settings, settings);
    assert_eq!(g.value("per_game")[game.to_string()], "path");
    assert!(g.text("heard").contains("loaded"), "{}", g.text("heard"));
    assert!(g.diagnostics().is_empty(), "{:?}", g.diagnostics());
}

/// A real, known brick lets native loads reach mini-game restoration.
fn saved_build_fixture() -> Game {
    Game::with_definitions(Definitions {
        entries: [(
            "probe:brick/plate".into(),
            bri_sim::testing::definition(
                "probe:brick/plate",
                [1, 1],
                1,
                bri_sim::definitions::Special::None,
                false,
            ),
        )]
        .into(),
    })
}

/// Exercise the actual native file framing before the authoritative load.
fn saved_build_snapshot(g: &mut Game, who: OwnerId) -> bri_world::build::SavedBuild {
    g.seq += 1;
    let bri_sim::session::Reply::Saved(build) =
        g.s.command(
            who,
            g.seq,
            Command::SaveBuild {
                events: true,
                ownership: false,
            },
        )
        .unwrap()
    else {
        panic!("Expected a saved build")
    };
    bri_world::build::decode(&bri_world::build::encode(&build).unwrap()).unwrap()
}

fn load_saved_build(g: &mut Game, who: OwnerId, build: bri_world::build::SavedBuild) {
    g.steps(5 * 120);
    g.send(
        who,
        Command::LoadBuild {
            build: Box::new(build),
            ownership: false,
        },
    )
    .unwrap();
    for _ in 0..120 {
        if !g.s.build_loading() {
            break;
        }
        g.steps(1);
    }
    assert!(!g.s.build_loading());
    // The package's loaded hook observes the restored state.
    g.steps(2);
}

fn saved_build_game(g: &mut Game, who: OwnerId, color: u8) -> u64 {
    if g.s.simulation().state().bricks.is_empty() {
        let mut world = g.s.simulation().state().clone();
        world.bricks.insert(
            1,
            bri_world::Brick::new(
                bri_world::ContentRef::Resolved("probe:brick/plate".into()),
                [20.25, 0.1, 20.25],
                who,
            ),
        );
        world.next_brick_id = 2;
        load_saved_build(g, who, bri_world::build::SavedBuild::new(world));
        assert_eq!(g.s.simulation().state().bricks.len(), 1);
    }
    g.send(
        who,
        Command::MiniGame(bri_sim::session::MiniGameRequest::Create {
            color,
            settings: bri_minigames::Settings {
                loadout: Default::default(),
                ..Default::default()
            },
        }),
    )
    .unwrap();
    g.s.minigame_views()
        .iter()
        .find(|game| game.color == color)
        .unwrap()
        .id
}

#[test]
fn saved_build_clears_absent_per_game_values_and_restores_explicit_empty_values() {
    let mut g = saved_build_fixture();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    let b = g.join(Vec3::new(4.0, 0.05, 0.0));
    let ga = saved_build_game(&mut g, a, 2).to_string();
    let gb = saved_build_game(&mut g, b, 3).to_string();
    let absent = saved_build_snapshot(&mut g, a);
    assert_eq!(absent.minigame.as_ref().unwrap()["packages"], json!({}));
    g.run(b, "keep_game", vec![PackageArg::String("neighbor".into())]);
    g.run(
        a,
        "keep_game",
        vec![PackageArg::String("later path".into())],
    );
    g.run(a, "reload", vec![]);

    load_saved_build(&mut g, a, absent);
    assert_eq!(g.value("per_game"), json!({ gb.clone(): "neighbor" }));
    assert_eq!(g.value("loaded_present"), json!(false));
    assert_eq!(g.value("loaded_value"), Value::Null);
    assert_eq!(g.value("reloads"), json!(1));

    // Empty text is authored data, distinct from an omitted game entry.
    g.run(a, "keep_game", vec![PackageArg::String(String::new())]);
    let mut empty = saved_build_snapshot(&mut g, a);
    // Unknown packages and ordinary keys cannot be written by a snapshot.
    let packages = &mut empty.minigame.as_mut().unwrap()["packages"];
    packages["probe"]["reloads"] = json!(99);
    packages["missing"] = json!({ "per_game": "ignored" });
    g.run(
        a,
        "keep_game",
        vec![PackageArg::String("later path".into())],
    );
    load_saved_build(&mut g, a, empty);
    assert_eq!(g.value("per_game"), json!({ ga: "", gb: "neighbor" }));
    assert_eq!(g.value("loaded_present"), json!(true));
    assert_eq!(g.value("loaded_value"), json!(""));
    assert_eq!(g.value("reloads"), json!(1));
    assert!(g.diagnostics().is_empty(), "{:?}", g.diagnostics());
}

#[test]
fn saved_build_rejects_a_merge_that_breaks_public_or_persistent_state_limits() {
    let mut g = saved_build_fixture();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    let b = g.join(Vec3::new(4.0, 0.05, 0.0));
    let ga = saved_build_game(&mut g, a, 2).to_string();
    let gb = saved_build_game(&mut g, b, 3).to_string();
    g.run(a, "keep_large_game", vec![]);
    let saved = saved_build_snapshot(&mut g, a);
    assert!(g.diagnostics().is_empty(), "{:?}", g.diagnostics());
    let authored = saved.minigame.as_ref().unwrap()["packages"]["probe"]["per_game"].clone();
    bri_package_runtime::state::check_value(&authored).unwrap();
    g.run(a, "keep_game", vec![PackageArg::String("current".into())]);
    g.run(b, "keep_large_game", vec![]);
    let before = g.value("per_game");
    let neighbor = before[&gb].clone();
    bri_package_runtime::state::check_value(&before).unwrap();
    let mut invalid = before.clone();
    invalid[&ga] = authored;
    assert!(bri_package_runtime::state::check_value(&invalid).is_err());

    load_saved_build(&mut g, a, saved);
    assert_eq!(g.value("per_game"), before, "rejection retains both games");
    assert_eq!(g.value("per_game")[&gb], neighbor);
    assert_eq!(g.value("loaded_present"), json!(true));
    assert_eq!(g.value("loaded_value"), json!("current"));
    assert!(
        g.diagnostics()
            .iter()
            .any(|d| d.contains("state.restore") && d.contains("per_game")),
        "{:?}",
        g.diagnostics()
    );
    g.s.package_state().validate().unwrap();
    let store = g.s.package_save().unwrap().store;
    assert_eq!(
        bri_package_runtime::Store::decode(&store.encode().unwrap()).unwrap(),
        store
    );
}

#[test]
fn saved_build_keeps_current_data_when_a_present_saved_value_is_malformed() {
    let mut g = saved_build_fixture();
    let a = g.join(Vec3::new(0.0, 0.05, 0.0));
    let game = saved_build_game(&mut g, a, 2).to_string();
    g.run(a, "keep_game", vec![PackageArg::String("current".into())]);
    let mut saved = saved_build_snapshot(&mut g, a);
    // This entry alone fits depth four, but its required game-id envelope
    // would exceed the same depth limit enforced on replicated/saved keys.
    let malformed = json!({"a": {"b": {"c": {"d": 1}}}});
    bri_package_runtime::state::check_value(&malformed).unwrap();
    saved.minigame.as_mut().unwrap()["packages"]["probe"]["per_game"] = malformed;
    load_saved_build(&mut g, a, saved);
    assert_eq!(g.value("per_game"), json!({ game: "current" }));
    assert_eq!(g.value("loaded_present"), json!(true));
    assert_eq!(g.value("loaded_value"), json!("current"));
    assert!(
        g.diagnostics()
            .iter()
            .any(|d| d.contains("state.restore") && d.contains("nest at most 4")),
        "{:?}",
        g.diagnostics()
    );
    g.s.package_state().validate().unwrap();
}
