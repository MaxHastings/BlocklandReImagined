//! Seams Add-On weapons lean on: magazines (`set_ammo` per mounted image),
//! taking a tool back, pickup-only items, the cancel key's command and a
//! state's arm animation.
//! The pack is written here; it is our own.
use bri_weapons::*;
use glam::Vec3;

struct Open;
impl Query for Open {
    fn sweep(&mut self, _: Vec3, _: Vec3, _: Filter) -> Option<Hit> {
        None
    }
    fn radius(&mut self, _: Vec3, _: f32, _: usize) -> Vec<Nearby> {
        Vec::new()
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        true
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        true
    }
}

const A: ActorId = ActorId(1);

/// Two magazine guns whose `Ready` state falls to `Empty` without ammo,
/// and an ammo box nobody holds.
fn pack() -> Pack {
    let gun = |name: &str| {
        format!(
            r#""kit:image/{name}": {{
                "states": [
                    {{ "name": "Activate", "ticks": 2, "timeout": 1 }},
                    {{ "name": "Ready", "no_ammo": 2, "down": 3 }},
                    {{ "name": "Empty", "ammo": 1 }},
                    {{ "name": "Fire", "ticks": 2, "timeout": 1 }}
                ],
                "commands": {{ "cancel": "kit:switch_round" }}
            }}"#
        )
    };
    let json = format!(
        r#"{{
            "schema_version": {SCHEMA},
            "id": "kit",
            "items": {{
                "kit:weapon/a": {{ "ui_name": "Gun A", "image": "kit:image/a" }},
                "kit:weapon/b": {{ "ui_name": "Gun B", "image": "kit:image/b" }},
                "kit:weapon/ammo": {{ "ui_name": "Ammo Box" }}
            }},
            "images": {{ {}, {} }}
        }}"#,
        gun("a"),
        gun("b")
    );
    Pack::from_json(json.as_bytes()).unwrap()
}

fn step(w: &mut WeaponsWorld, ticks: usize) {
    for _ in 0..ticks {
        w.step(&mut Open);
    }
}

fn state(w: &WeaponsWorld) -> String {
    w.image_state(A, 0).unwrap().1.name.clone()
}

#[test]
fn the_next_gun_mounts_loaded_after_emptying_one() {
    let mut w = WeaponsWorld::new(pack()).unwrap();
    w.add_actor(A, 5).unwrap();
    let a = w.give(A, "kit:weapon/a").unwrap();
    let b = w.give(A, "kit:weapon/b").unwrap();
    w.equip(A, Some(a)).unwrap();
    step(&mut w, 10);
    assert_eq!(state(&w), "Ready");
    // The rules empty gun A's magazine.
    w.set_ammo(A, false).unwrap();
    step(&mut w, 2);
    assert_eq!(state(&w), "Empty");
    // Gun B comes out loaded, as `mountImage` and `WeaponImage::onMount`
    // left every freshly mounted image.
    w.equip(A, Some(b)).unwrap();
    step(&mut w, 10);
    assert_eq!(state(&w), "Ready");
    // And gun A, drawn again, is loaded too until the rules say otherwise.
    w.equip(A, Some(a)).unwrap();
    step(&mut w, 10);
    assert_eq!(state(&w), "Ready");
    w.set_ammo(A, false).unwrap();
    step(&mut w, 2);
    assert_eq!(state(&w), "Empty");
}

#[test]
fn take_item_prefers_the_held_slot_and_puts_it_away() {
    let mut w = WeaponsWorld::new(pack()).unwrap();
    w.add_actor(A, 5).unwrap();
    w.give_at(A, 0, "kit:weapon/a").unwrap();
    w.give_at(A, 3, "kit:weapon/a").unwrap();
    w.equip(A, Some(3)).unwrap();
    step(&mut w, 4);
    assert_eq!(w.take_item(A, "kit:weapon/a").unwrap(), Some(3));
    assert!(w.image_state(A, 0).is_none(), "the held gun was put away");
    assert_eq!(w.actor(A).unwrap().selected, None);
    assert_eq!(w.take_item(A, "kit:weapon/a").unwrap(), Some(0));
    assert_eq!(w.take_item(A, "kit:weapon/a").unwrap(), None);
    assert!(w.actor(A).unwrap().inventory.iter().all(Option::is_none));
}

#[test]
fn an_item_without_an_image_is_carried_but_holds_nothing() {
    let mut w = WeaponsWorld::new(pack()).unwrap();
    w.add_actor(A, 5).unwrap();
    let drop = w
        .spawn_drop("kit:weapon/ammo", Vec3::new(0.0, 1.0, 0.0), Vec3::ZERO)
        .unwrap();
    assert!(w.pickup_ready(A, drop));
    let slot = w.pickup(A, drop).unwrap();
    w.equip(A, Some(slot)).unwrap();
    step(&mut w, 4);
    assert!(w.image_state(A, 0).is_none());
    assert_eq!(w.actor(A).unwrap().selected, Some(slot));
    // A drop the rules use up where it lies goes without anyone taking it.
    let drop = w
        .spawn_drop("kit:weapon/ammo", Vec3::ZERO, Vec3::ZERO)
        .unwrap();
    w.step(&mut Open);
    assert!(w.remove_drop(drop));
    assert!(!w.remove_drop(drop));
    assert!(w.drops().all(|d| d.id != drop));
    assert!(!w.pickup_ready(A, drop));
}

#[test]
fn the_cancel_key_command_is_validated_and_found() {
    let pack = pack();
    let commands = &pack.images["kit:image/a"].commands;
    assert_eq!(commands.cancel.as_deref(), Some("kit:switch_round"));
    assert!(commands.runs("kit:switch_round"));
    assert!(!commands.runs("kit:fire"));
    let mut bad = pack.clone();
    bad.images.get_mut("kit:image/a").unwrap().commands.cancel = Some("no package".into());
    assert!(bad.validate().is_err());
}

#[test]
fn a_nameless_item_is_refused_with_its_id() {
    let mut bad = pack();
    bad.items.get_mut("kit:weapon/b").unwrap().ui_name = " ".into();
    let error = format!("{:#}", bad.validate().unwrap_err());
    assert!(error.contains("kit:weapon/b"), "{error}");
    assert!(error.contains("ui_name"), "{error}");
}

/// A kit whose gun throws its own casing (`casing`, a `DebrisData` with a
/// model) and whose rifle throws the base game's.
pub fn casing_kit() -> Pack {
    let def = |name: &str, class: &str, fields: serde_json::Value| {
        serde_json::json!({
            "name": name, "class": class, "parent": null,
            "source": { "path": "Add-Ons/Weapon_Kit/kit.cs", "sha256": "0".repeat(64), "line": 1 },
            "fields": fields,
        })
    };
    let json = serde_json::json!({
        "schema_version": 3,
        "id": "kit",
        "items": {},
        "images": {
            "kit:image/gun": { "name": "kitGunImage", "casing": "kitShellDebris",
                "states": [{ "name": "Ready" }] },
            "kit:image/rifle": { "name": "kitRifleImage", "casing": "gunShellDebris",
                "states": [{ "name": "Ready" }] },
            "kit:image/plain": { "name": "kitPlainImage", "states": [{ "name": "Ready" }] }
        },
        "definitions": [
            def("kitShellDebris", "DebrisData", serde_json::json!({
                "shapefile": "\"./shell.dts\"", "lifetime": "1.5", "numbounces": "2",
                "gravmodifier": "3", "elasticity": "0.4", "staticonmaxbounce": "true",
                "minspinspeed": "90", "maxspinspeed": "-30" })),
            def("gunShellDebris", "DebrisData", serde_json::json!({
                "shapefile": "\"./gunshell.dts\"" })),
            def("kitGunImage", "ShapeBaseImageData", serde_json::json!({
                "shellexitdir": "\"0 0 1\"", "shellexitoffset": "\"0 0.5 0\"",
                "shellvelocity": "4", "shellexitvariance": "5" })),
        ]
    });
    Pack::from_json(&serde_json::to_vec(&json).unwrap()).unwrap()
}

#[test]
fn an_images_casing_reads_its_debris_and_shell_fields() {
    let casings = bri_weapons::debris::casings(&casing_kit());
    let gun = &casings["kit:image/gun"];
    assert_eq!(gun.debris.model, "Add-Ons/Weapon_Kit/shell.dts");
    assert_eq!(
        (gun.debris.lifetime, gun.debris.bounces, gun.debris.gravity),
        (1.5, 2, 3.0)
    );
    assert!(gun.debris.static_on_max_bounce);
    assert_eq!(gun.debris.spin, [-30.0, 90.0]);
    // Torque z-up "x y z" is native (x, z, -y).
    assert_eq!(gun.exit_direction, [0.0, 1.0, 0.0]);
    assert_eq!(gun.exit_offset, [0.0, 0.0, -0.5]);
    assert_eq!((gun.velocity, gun.exit_variance), (4.0, 5.0));
    // No image fields: ShapeBaseImageData's defaults.
    let rifle = &casings["kit:image/rifle"];
    assert_eq!(rifle.exit_direction, [1.0, 1.0, 0.0]);
    assert_eq!((rifle.velocity, rifle.exit_variance), (1.0, 20.0));
    assert!(!casings.contains_key("kit:image/plain"));
}

#[test]
fn a_states_arm_plays_on_the_holders_arm_thread() {
    let json = format!(
        r#"{{ "schema_version": {SCHEMA}, "id": "kit",
            "items": {{ "kit:weapon/pick": {{ "ui_name": "Pick", "image": "kit:image/pick" }} }},
            "images": {{ "kit:image/pick": {{ "states": [
                {{ "name": "Activate", "ticks": 2, "timeout": 1 }},
                {{ "name": "Ready", "down": 2 }},
                {{ "name": "Swing", "ticks": 3, "timeout": 3, "arm": "armattack" }},
                {{ "name": "Done", "ticks": 3, "timeout": 1, "arm": "root" }}
            ] }} }} }}"#
    );
    let mut w = WeaponsWorld::new(Pack::from_json(json.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    let pick = w.give(A, "kit:weapon/pick").unwrap();
    w.equip(A, Some(pick)).unwrap();
    step(&mut w, 10);
    w.trigger(A, true).unwrap();
    let arms: Vec<String> = (0..12)
        .flat_map(|_| w.step(&mut Open))
        .filter_map(|e| match e {
            Event::Animation {
                actor: A,
                thread: 2,
                sequence,
                image_hand: None,
            } => Some(sequence),
            _ => None,
        })
        .collect();
    assert_eq!(arms[..2], ["armattack", "root"], "{arms:?}");
}

/// A gun whose `onFire` runs a rule command (a magazine counting rounds)
/// still fires its projectile, once per shot, as v20's
/// `Parent::onFire` did; a tool with no projectile only runs the command.
#[test]
fn an_on_fire_command_counts_the_shot_and_the_round_still_flies() {
    let json = format!(
        r#"{{
            "schema_version": {SCHEMA},
            "id": "kit",
            "items": {{
                "kit:weapon/rifle": {{ "ui_name": "Rifle", "image": "kit:image/rifle" }},
                "kit:weapon/tool": {{ "ui_name": "Tool", "image": "kit:image/tool" }}
            }},
            "images": {{
                "kit:image/rifle": {{
                    "projectile": "kit:projectile/round",
                    "commands": {{ "states": {{ "onfire": "kit:fired" }} }},
                    "states": [
                        {{ "name": "Activate", "ticks": 2, "timeout": 1 }},
                        {{ "name": "Ready", "down": 2 }},
                        {{ "name": "Fire", "ticks": 10, "script": "onFire", "timeout": 3 }},
                        {{ "name": "Hold", "up": 1 }}
                    ]
                }},
                "kit:image/tool": {{
                    "commands": {{ "states": {{ "onfire": "kit:use" }} }},
                    "states": [
                        {{ "name": "Activate", "ticks": 2, "timeout": 1 }},
                        {{ "name": "Ready", "down": 2 }},
                        {{ "name": "Fire", "ticks": 10, "script": "onFire", "timeout": 3 }},
                        {{ "name": "Hold", "up": 1 }}
                    ]
                }}
            }},
            "projectiles": {{
                "kit:projectile/round": {{ "speed": 100.0, "lifetime_ticks": 240, "fade_ticks": 240 }}
            }}
        }}"#
    );
    let mut w = WeaponsWorld::new(Pack::from_json(json.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    for (item, command, rounds) in [
        ("kit:weapon/rifle", "kit:fired", 1),
        ("kit:weapon/tool", "kit:use", 0),
    ] {
        let slot = w.give(A, item).unwrap();
        w.equip(A, Some(slot)).unwrap();
        step(&mut w, 10);
        let before = w.projectiles().count();
        w.trigger(A, true).unwrap();
        let mut commands = 0;
        for _ in 0..8 {
            commands += w
                .step(&mut Open)
                .iter()
                .filter(|e| matches!(e, Event::ToolFire { command: Some(c), .. } if c == command))
                .count();
        }
        w.trigger(A, false).unwrap();
        step(&mut w, 20);
        assert_eq!(commands, 1, "{item}: the command runs once a shot");
        assert_eq!(
            w.projectiles().count() - before,
            rounds,
            "{item}: rounds in flight"
        );
        w.take_item(A, item).unwrap();
    }
}
