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
    session::{BrickHand, Command, ControlObject, Notice, PackageArg, PackageCommand, Session},
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
fn cmd_orbit(p, target, distance) { orbit_camera(p, target, distance); }
fn cmd_orbit_back(p) { orbit_camera(p, ()); }
fn cmd_put_away(p) { unmount_image(p); }
"#;

fn behaviour() -> Value {
    let command = |name: &str, args: &[&str]| json!({ "name": name, "args": args });
    json!({
        "schema_version": 1,
        "script": "main.rhai",
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
            command("orbit", &["int", "float"]),
            command("orbit_back", &[]),
            command("put_away", &[]),
        ],
        "state": { "global": {
            "hit": { "default": "", "visible": "everyone" },
            "distance": { "default": 0.0, "visible": "everyone" },
            "region": { "default": "", "visible": "everyone" },
            "can": { "default": "", "visible": "everyone" },
            "facts": { "default": "", "visible": "everyone" },
            "center": { "default": 0.0, "visible": "everyone" },
            "reloads": { "default": 0, "visible": "everyone" },
            "env": { "default": "", "visible": "everyone" }
        } }
    })
}

/// A gun whose Ready state goes to Empty without ammo, which takes the
/// light key for its reload command, and a scope image to swap in.
fn weapons() -> bri_weapons::Pack {
    let pack = json!({
        "schema_version": 3,
        "id": "probe",
        "items": { "probe:weapon/gun": { "ui_name": "Probe Gun", "image": "probe:image/gun" } },
        "images": {
            "probe:image/gun": {
                "commands": { "light": "probe:reload" },
                "states": [
                    { "name": "Activate", "ticks": 6, "timeout": 1 },
                    { "name": "Ready", "no_ammo": 2 },
                    { "name": "Empty", "ammo": 1 }
                ]
            },
            "probe:image/scope": { "states": [{ "name": "Scoped" }] }
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
        "capabilities": ["damage", "effects", "player", "lighting", "environment"],
        "provides": [
            { "kind": "behaviour", "id": "probe:behaviour/main", "file": "behaviour.json" },
            { "kind": "script", "id": "probe:script/main", "file": "main.rhai" }
        ]
    });
    std::fs::write(dir.join("package.json"), manifest.to_string()).unwrap();
    std::fs::write(dir.join("behaviour.json"), behaviour().to_string()).unwrap();
    std::fs::write(dir.join("main.rhai"), SCRIPT).unwrap();
    let set = PackageSet {
        schema_version: 1,
        packages: vec![PackageEntry {
            id: "probe".into(),
            version: "1.0.0".into(),
            side: Side::Server,
            dir: "probe".into(),
            role: None,
        }],
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
        let ground = ColliderBuilder::cuboid(100.0, 0.5, 100.0)
            .translation(Vector::new(0.0, -0.5, 0.0))
            .user_data(u128::MAX);
        let mut s = Session::new(
            Simulation::new(
                World::new("Probe".into(), "probe".into(), vec![[1.0; 4]]),
                Definitions::default(),
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
        g.run(a, "region", vec![PackageArg::Int(b as i64), PackageArg::Float(y)]);
        assert_eq!(g.text("region"), region, "{y}");
    }
    g.run(a, "region", vec![PackageArg::Int(999), PackageArg::Float(1.0)]);
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
        g.diagnostics().iter().any(|d| d.contains("more than 64 rays")),
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
    g.run(a, "hurt", vec![PackageArg::Int(b as i64), PackageArg::String("ProbeShot".into())]);
    g.run(a, "hurt", vec![PackageArg::Int(b as i64), PackageArg::String(String::new())]);
    assert!((g.s.vitals()[&b].health - 40.0).abs() < 0.01);
    // An unknown damage type is refused and hurts nobody.
    g.run(a, "hurt", vec![PackageArg::Int(b as i64), PackageArg::String("Nope".into())]);
    assert!((g.s.vitals()[&b].health - 40.0).abs() < 0.01);
    assert!(
        g.diagnostics().iter().any(|d| d.contains("No damage type `Nope`")),
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
    assert!((1.2..1.4).contains(&center), "half the standing body: {center}");
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
    g.run(a, "scope", vec![PackageArg::String("probe:image/scope".into())]);
    g.steps(1);
    g.run(a, "facts", vec![]);
    assert_eq!(g.text("facts"), "3|probe:image/scope|Scoped|false|1.0");
    g.run(a, "scope", vec![PackageArg::String(String::new())]);
    g.steps(20);
    g.run(a, "facts", vec![]);
    assert_eq!(g.text("facts"), "3|probe:image/gun|Ready|false|1.0");
    // Another package's image is refused.
    g.run(a, "scope", vec![PackageArg::String("other:image/scope".into())]);
    g.steps(1);
    g.run(a, "facts", vec![]);
    assert_eq!(g.text("facts"), "3|probe:image/gun|Ready|false|1.0");
    assert!(
        g.diagnostics().iter().any(|d| d.contains("is not an image of `probe`")),
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
    assert!(g.s.vitals()[&a].light, "with nothing in hand the light toggles");
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
    let fovs: Vec<Option<f32>> = g
        .s
        .take_private_notices()
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
    g.run(a, "lamp", vec![PackageArg::Float(4.0), PackageArg::Float(3.0), PackageArg::Bool(true)]);
    g.run(a, "lamp", vec![PackageArg::Float(-4.0), PackageArg::Float(3.0), PackageArg::Bool(false)]);
    let rules = g.s.map_light_rules();
    assert_eq!(
        rules,
        [
            MapLightRule { position: [4.0, 2.0, 0.0], radius: 3.0, tint: [2.0, 1.0, 0.5] },
            MapLightRule { position: [-4.0, 2.0, 0.0], radius: 3.0, tint: [0.0; 3] },
        ]
    );
    // What a light inside, outside or between the spheres takes.
    assert_eq!(MapLightRule::tint_at(&rules, Vec3::new(4.0, 3.0, 0.0)), Vec3::new(2.0, 1.0, 0.5));
    assert_eq!(MapLightRule::tint_at(&rules, Vec3::new(-5.0, 2.0, 0.0)), Vec3::ZERO);
    assert_eq!(MapLightRule::tint_at(&rules, Vec3::new(0.0, 2.0, 0.0)), Vec3::ONE);
    // The same sphere again replaces its rule (so repeated calls never
    // pile up), and an empty map puts the lights back as the map was lit.
    g.run(a, "lamp_reset", vec![PackageArg::Float(4.0), PackageArg::Float(3.0)]);
    let rules = g.s.map_light_rules();
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[1], MapLightRule { position: [4.0, 2.0, 0.0], radius: 3.0, tint: [1.0; 3] });
    assert_eq!(MapLightRule::tint_at(&rules, Vec3::new(4.0, 2.0, 0.0)), Vec3::ONE);
    // A later, wider sphere wins where it overlaps.
    g.run(a, "lamp", vec![PackageArg::Float(0.0), PackageArg::Float(10.0), PackageArg::Bool(false)]);
    assert_eq!(MapLightRule::tint_at(&g.s.map_light_rules(), Vec3::new(4.0, 2.0, 0.0)), Vec3::ZERO);
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
    assert_eq!(e.sun_flare, Some(SunFlare { size: 2.0, ..SunFlare::default() }));
    // The cycle starts at the time of day set, from the tick it was set.
    let cycle = e.day_cycle.unwrap();
    assert_eq!((cycle.length_seconds, cycle.time, cycle.anchor_tick), (60.0, 0.25, tick));
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
            distance: 6
        }
    );
    assert!(
        g.send(a, Command::ControlPlayer).is_err(),
        "the Add-On's to end"
    );
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
