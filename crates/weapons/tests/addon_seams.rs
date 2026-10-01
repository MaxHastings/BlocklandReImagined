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

/// A v20 shotgun's `onFire` fired its pellets, then a close blast of its
/// own projectile: `volleys`. Each volley flies at its projectile's speed
/// along the aim, within its own spread, after the shot's recoil.
#[test]
fn a_volley_fires_its_own_projectile_after_the_pellets() {
    let json = format!(
        r#"{{
            "schema_version": {SCHEMA},
            "id": "kit",
            "items": {{ "kit:weapon/shotgun": {{ "ui_name": "Shotgun", "image": "kit:image/shotgun" }} }},
            "images": {{
                "kit:image/shotgun": {{
                    "projectile": "kit:projectile/pellet",
                    "shot": {{ "projectiles": 6, "spread": 0.004, "recoil": 3.0 }},
                    "volleys": [ {{ "projectile": "kit:projectile/blast", "projectiles": 1, "spread": 0.0 }} ],
                    "states": [
                        {{ "name": "Activate", "ticks": 2, "timeout": 1 }},
                        {{ "name": "Ready", "down": 2 }},
                        {{ "name": "Fire", "ticks": 10, "script": "onFire", "timeout": 3 }},
                        {{ "name": "Hold", "up": 1 }}
                    ]
                }}
            }},
            "projectiles": {{
                "kit:projectile/pellet": {{ "speed": 100.0, "inherit": 1.0, "lifetime_ticks": 240, "fade_ticks": 240 }},
                "kit:projectile/blast": {{ "speed": 40.0, "inherit": 1.0, "lifetime_ticks": 240, "fade_ticks": 240 }}
            }}
        }}"#
    );
    let mut w = WeaponsWorld::new(Pack::from_json(json.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    let slot = w.give(A, "kit:weapon/shotgun").unwrap();
    w.equip(A, Some(slot)).unwrap();
    step(&mut w, 10);
    w.trigger(A, true).unwrap();
    let mut spawned = vec![];
    for _ in 0..8 {
        for e in w.step(&mut Open) {
            if let Event::Spawned {
                definition,
                velocity,
                ..
            } = e
            {
                spawned.push((definition, velocity));
            }
        }
    }
    let pellets = spawned
        .iter()
        .filter(|(d, _)| d == "kit:projectile/pellet")
        .count();
    let blasts: Vec<_> = spawned
        .iter()
        .filter(|(d, _)| d == "kit:projectile/blast")
        .collect();
    assert_eq!((pellets, blasts.len()), (6, 1));
    // Straight along the aim at 40, less the recoil of 3 it inherits.
    let aim = Frame::default().direction.normalize();
    assert!(
        (blasts[0].1 - aim * 37.0).length() < 1e-3,
        "{:?}",
        blasts[0].1
    );

    // A volley of a projectile the pack lacks is refused.
    let bad = json.replace(
        "kit:projectile/blast\", \"projectiles\"",
        "kit:projectile/none\", \"projectiles\"",
    );
    assert!(Pack::from_json(bad.as_bytes()).is_err());
}

/// A two-barrel gun whose script fired both barrels while it had more than
/// two rounds and its single barrel with whatever was left: `per_shot` 2,
/// `last_rounds` 2 and a `last_shot`. With 5 rounds it fires 8 pellets
/// twice (3, then 1 left), then the single barrel's 4 with the last one,
/// then clicks.
#[test]
fn a_magazines_last_rounds_fire_the_last_shot() {
    let json = format!(
        r#"{{
            "schema_version": {SCHEMA},
            "id": "kit",
            "items": {{ "kit:weapon/pair": {{ "ui_name": "Pair", "image": "kit:image/pair" }} }},
            "images": {{
                "kit:image/pair": {{
                    "projectile": "kit:projectile/pellet",
                    "shot": {{ "projectiles": 8, "spread": 0.004 }},
                    "last_shot": {{ "shot": {{ "projectiles": 4, "spread": 0.002 }} }},
                    "magazine": {{ "size": 5, "ammo": "shells", "per_shot": 2, "last_rounds": 2,
                                   "reload_ticks": 600, "reserve": 0 }},
                    "states": [
                        {{ "name": "Activate", "ticks": 2, "timeout": 1 }},
                        {{ "name": "Ready", "down": 2, "no_ammo": 4 }},
                        {{ "name": "Fire", "ticks": 10, "script": "onFire", "timeout": 3 }},
                        {{ "name": "Hold", "up": 1 }},
                        {{ "name": "Empty", "ammo": 1 }}
                    ]
                }}
            }},
            "projectiles": {{
                "kit:projectile/pellet": {{ "speed": 100.0, "inherit": 1.0, "lifetime_ticks": 240, "fade_ticks": 240 }}
            }}
        }}"#
    );
    let mut w = WeaponsWorld::new(Pack::from_json(json.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    let slot = w.give(A, "kit:weapon/pair").unwrap();
    w.equip(A, Some(slot)).unwrap();
    step(&mut w, 10);
    let mut pulls = vec![];
    for _ in 0..4 {
        w.trigger(A, true).unwrap();
        let mut pellets = 0;
        for _ in 0..20 {
            pellets += w
                .step(&mut Open)
                .iter()
                .filter(|e| matches!(e, Event::Spawned { .. }))
                .count();
        }
        w.trigger(A, false).unwrap();
        step(&mut w, 5);
        pulls.push((pellets, w.ammo(A).unwrap().rounds));
    }
    assert_eq!(pulls, [(8, 3), (8, 1), (4, 0), (0, 0)]);

    // A last shot needs a magazine that names its rounds.
    let bad = json.replace(r#""per_shot": 2, "last_rounds": 2,"#, r#""per_shot": 2,"#);
    assert!(Pack::from_json(bad.as_bytes()).is_err());
}

/// A frag grenade's script threw two kinds of fragment as it burst
/// (shrapnel and smoke trails): `children` as a list, each set its own
/// count, speed and directions.
#[test]
fn a_projectile_bursts_into_several_sets_of_children() {
    let json = format!(
        r#"{{
            "schema_version": {SCHEMA},
            "id": "kit",
            "items": {{}},
            "images": {{}},
            "projectiles": {{
                "kit:projectile/frag": {{ "speed": 10.0, "lifetime_ticks": 5, "fade_ticks": 5,
                    "explode_death": true,
                    "children": [
                        {{ "projectile": "kit:projectile/shard", "count": 5, "speed": 20.0, "on_explode": true }},
                        {{ "projectile": "kit:projectile/trail", "count": 3, "speed": 40.0, "on_explode": true }}
                    ] }},
                "kit:projectile/shard": {{ "speed": 20.0, "lifetime_ticks": 60, "fade_ticks": 60 }},
                "kit:projectile/trail": {{ "speed": 40.0, "lifetime_ticks": 60, "fade_ticks": 60 }}
            }}
        }}"#
    );
    let mut w = WeaponsWorld::new(Pack::from_json(json.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    w.spawn("kit:projectile/frag", A, Vec3::ZERO, Vec3::X * 10.0, 1.0)
        .unwrap();
    let mut spawned = vec![];
    for _ in 0..20 {
        for e in w.step(&mut Open) {
            if let Event::Spawned {
                definition,
                velocity,
                ..
            } = e
            {
                spawned.push((definition, velocity.length()));
            }
        }
    }
    let of = |name: &str| {
        spawned
            .iter()
            .filter(|(d, _)| d == name)
            .map(|(_, speed)| *speed)
            .collect::<Vec<_>>()
    };
    let (shards, trails) = (of("kit:projectile/shard"), of("kit:projectile/trail"));
    assert_eq!((shards.len(), trails.len()), (5, 3), "{spawned:?}");
    assert!(shards.iter().all(|s| (s - 20.0).abs() < 1e-3));
    assert!(trails.iter().all(|s| (s - 40.0).abs() < 1e-3));
}

/// A heavy gun's fire states each ran a script of their own (`onFire2`,
/// `onFire3`) with its own spread and recoil, every round made larger:
/// `state_shots` fire on entering those states, each taking its round.
#[test]
fn a_fire_state_of_its_own_fires_its_own_shot() {
    let json = format!(
        r#"{{
            "schema_version": {SCHEMA},
            "id": "kit",
            "items": {{ "kit:weapon/heavy": {{ "ui_name": "Heavy", "image": "kit:image/heavy" }} }},
            "images": {{
                "kit:image/heavy": {{
                    "projectile": "kit:projectile/round",
                    "shot": {{ "projectiles": 1, "recoil": 1.0, "scale": 1.5 }},
                    "state_shots": {{
                        "onfire2": {{ "projectiles": 1, "spread": 0.003, "recoil": 0.5, "scale": 1.5 }},
                        "onfire3": {{ "projectiles": 1, "spread": 0.004, "recoil": 0.25, "scale": 1.5 }}
                    }},
                    "magazine": {{ "size": 4, "ammo": "heavy", "reload_ticks": 600, "reserve": 0 }},
                    "states": [
                        {{ "name": "Activate", "ticks": 2, "timeout": 1 }},
                        {{ "name": "Ready", "down": 2, "no_ammo": 5 }},
                        {{ "name": "Fire", "ticks": 4, "script": "onFire", "timeout": 3 }},
                        {{ "name": "Fire2", "ticks": 4, "script": "onFire2", "timeout": 4, "up": 1, "no_ammo": 5,
                           "arm": "shiftright", "gesture": "shiftleft" }},
                        {{ "name": "Fire3", "ticks": 4, "script": "onFire3", "timeout": 4, "up": 1, "no_ammo": 5 }},
                        {{ "name": "Empty", "ammo": 1 }}
                    ]
                }}
            }},
            "projectiles": {{
                "kit:projectile/round": {{ "speed": 100.0, "inherit": 1.0, "lifetime_ticks": 240, "fade_ticks": 240,
                    "damage": 10.0, "fixed_damage": true }}
            }}
        }}"#
    );
    let mut w = WeaponsWorld::new(Pack::from_json(json.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    let slot = w.give(A, "kit:weapon/heavy").unwrap();
    w.equip(A, Some(slot)).unwrap();
    step(&mut w, 10);
    w.trigger(A, true).unwrap();
    let mut recoils = vec![];
    let mut moves = vec![];
    for _ in 0..40 {
        for e in w.step(&mut Open) {
            match e {
                Event::Recoil { velocity, .. } => recoils.push(velocity.length()),
                Event::Animation {
                    thread, sequence, ..
                } if thread >= 2 => moves.push((thread, sequence)),
                _ => {}
            }
        }
    }
    // Fire2's arm move and the other hand's, on thread 3.
    assert_eq!(
        moves,
        [(2, "shiftright".to_string()), (3, "shiftleft".to_string())]
    );
    // onFire, onFire2, then onFire3 until the magazine is empty.
    assert_eq!(recoils, [1.0, 0.5, 0.25, 0.25]);
    assert_eq!(w.ammo(A).unwrap().rounds, 0);
    assert!(w.projectiles().count() == 4 && w.projectiles().all(|p| p.scale == 1.5));

    // A state shot's script is lowercase and not onfire's.
    let bad = json.replace(r#""onfire2": {"#, r#""onfire": {"#);
    assert!(Pack::from_json(bad.as_bytes()).is_err());
}

/// A reload state's timed cues: an arm move and a sound at their own
/// milliseconds after the state began (v20's `%obj.schedule(450,
/// "playThread", 2, plant)` and `schedule(650, 0, serverPlay3D, ...)`),
/// each played even once the state has moved on, and the sound where the
/// holder stood as it began. A new body drops the animations still due.
#[test]
fn a_states_cues_play_at_their_times_after_it_begins() {
    let json = format!(
        r#"{{ "schema_version": {SCHEMA}, "id": "kit",
            "sounds": {{ "kit:sound/tap": {{ "file": "tap.wav" }} }},
            "items": {{ "kit:weapon/gun": {{ "ui_name": "Gun", "image": "kit:image/gun" }} }},
            "images": {{ "kit:image/gun": {{ "states": [
                {{ "name": "Activate", "ticks": 2, "timeout": 1 }},
                {{ "name": "Ready", "down": 2 }},
                {{ "name": "Reload", "ticks": 12, "timeout": 3, "cues": [
                    {{ "thread": 0, "sequence": "shiftright" }},
                    {{ "after_ms": 100, "thread": 2, "sequence": "plant" }},
                    {{ "after_ms": 250, "sound": "kit:sound/tap" }}
                ] }},
                {{ "name": "Done", "up": 1 }}
            ] }} }} }}"#
    );
    let mut w = WeaponsWorld::new(Pack::from_json(json.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    let gun = w.give(A, "kit:weapon/gun").unwrap();
    w.equip(A, Some(gun)).unwrap();
    step(&mut w, 10);
    let mut frame = Frame {
        position: Vec3::new(1.0, 0.0, 0.0),
        ..Default::default()
    };
    w.set_frame(A, frame.clone()).unwrap();
    w.trigger(A, true).unwrap();
    let mut seen = vec![];
    for tick in 0..40 {
        if tick == 5 {
            // The holder walks on; the sound stays where the reload began.
            frame.position = Vec3::new(9.0, 0.0, 0.0);
            w.set_frame(A, frame.clone()).unwrap();
        }
        for e in w.step(&mut Open) {
            match e {
                Event::Animation {
                    thread, sequence, ..
                } if thread != 2 || sequence == "plant" => seen.push((tick, format!("{thread} {sequence}"))),
                Event::Sound { profile, position, .. } if profile == "kit:sound/tap" => {
                    assert_eq!(position, Vec3::new(1.0, 0.0, 0.0));
                    seen.push((tick, profile));
                }
                _ => {}
            }
        }
    }
    // 100 ms is 12 ticks and 250 ms 30, counted from the state's tick.
    assert_eq!(
        seen,
        [
            (0, "0 shiftright".to_string()),
            (12, "2 plant".to_string()),
            (30, "kit:sound/tap".to_string())
        ],
        "{seen:?}"
    );

    // A cue still due when the holder gets a new body keeps its sound only.
    w.trigger(A, false).unwrap();
    step(&mut w, 30);
    w.trigger(A, true).unwrap();
    step(&mut w, 2);
    w.respawned(A).unwrap();
    let later: Vec<Event> = (0..40).flat_map(|_| w.step(&mut Open)).collect();
    assert!(
        !later.iter().any(|e| matches!(e, Event::Animation { sequence, .. } if sequence == "plant")),
        "{later:?}"
    );
    assert!(later.iter().any(|e| matches!(e, Event::Sound { profile, .. } if profile == "kit:sound/tap")));
}
