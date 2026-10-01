//! Brick damage against v20: rockets knock bricks out (fake kills that
//! respawn) under the LAN, minigame and ownership rules of
//! `ProjectileData::onExplode`; the hammer deletes them for good; and every
//! brick death is announced for client debris. See docs/audits/brick-damage.md.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_minigames::Settings;
use bri_sim::{
    definitions::{Definition, Definitions},
    player::{MoveInput, PlayerState, PlayerTuning},
    presentation::CueKind,
    session::{ActionAim, Command, MiniGameRequest, Reply, Session},
    simulation::Simulation,
};
use bri_world::{ContentRef, EventRow, EventTarget, EventValue, World, authority::Edit};
use glam::Vec3;
use rapier3d::prelude::*;
mod common;
use common::{move_sequence, swing, weapon_pack};

const HZ: u64 = bri_weapons::TICK_HZ as u64;
const ROCKET: &str = "v20.weapon.rocketlauncheritem";
const HAMMER: &str = "v20.weapon.hammeritem";

fn session() -> Session {
    let mesh = Mesh {
        schema_version: 1,
        id: "brick".into(),
        footprint_studs: [2, 2],
        height_plates: 3,
        attachment_rows: vec!["bb".into(); 6],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "brick".into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [1.0, 0.6, 1.0],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    let definitions = Definitions {
        entries: [(
            "brick".into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
                reflection: None,
                link: None,
                glass: [0.0; 4],
            },
        )]
        .into(),
    };
    let mut s = Session::new(
        Simulation::new(
            World::new("Bricks".into(), "test".into(), vec![[1.0; 4]; 2]),
            definitions,
            vec![
                ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
    s
}

fn plant(s: &mut Session, owner: u64, seq: u64, position: [f32; 3]) -> u64 {
    let Reply::Planted(id) = s
        .command(
            owner,
            seq,
            Command::Plant {
                definition: "brick".into(),
                position,
                quarter_turns: 0,
                color: 1,
            },
        )
        .unwrap()
    else {
        panic!("expected plant")
    };
    id
}

fn kills(s: &mut Session) -> Vec<(u64, [f32; 3], f32)> {
    s.take_cues()
        .into_iter()
        .filter_map(|c| match c.kind {
            CueKind::BrickKill {
                brick,
                definition,
                color,
                origin,
                force,
                ..
            } => {
                assert_eq!(definition, ContentRef::Resolved("brick".into()));
                assert_eq!(color, 1);
                Some((brick, origin, force))
            }
            _ => None,
        })
        .collect()
}

/// Face `target` from the feet, then pitch from the eye that yaw puts in
/// place, and refresh the input lease so triggers are not dropped.
fn aim_at(s: &mut Session, owner: u64, target: Vec3) -> ActionAim {
    let p = s
        .snapshot()
        .players
        .into_iter()
        .find(|p| p.owner == owner)
        .unwrap();
    let flat = target - Vec3::from(p.feet);
    let yaw = flat.x.atan2(-flat.z);
    let facing = PlayerState { yaw, ..p };
    let d = target - facing.eye(&PlayerTuning::default());
    let aim = ActionAim {
        yaw,
        pitch: d.y.atan2(Vec3::new(d.x, 0.0, d.z).length()),
    };
    s.movement(
        owner,
        move_sequence(s),
        MoveInput {
            yaw: aim.yaw,
            pitch: aim.pitch,
            ..Default::default()
        },
    )
    .unwrap();
    aim
}

fn with_weapons(lan: bool) -> Session {
    let mut s = session();
    s.set_lan_host(lan);
    s.set_weapon_pack(weapon_pack()).unwrap();
    s
}

/// One rocket fired at two bricks eight metres away.
struct Rocket {
    /// Single player and LAN (`$Server::LAN`), or an internet server.
    lan: bool,
    /// The bricks belong to a bystander who never joins a minigame.
    bystander_bricks: bool,
    /// The shooter's minigame, if any.
    game: Option<Settings>,
    /// The shooter (an admin) presses F8 while the rocket is in flight.
    drop_in_flight: bool,
}

struct Blast {
    s: Session,
    bricks: [u64; 2],
    /// Bricks thrown as debris, with the tick they were thrown.
    thrown: Vec<(u64, u64)>,
}

impl Blast {
    fn standing(&self, id: u64) -> bool {
        let b = &self.s.simulation().state().bricks[&id];
        b.visible && b.colliding && b.raycast
    }
    fn assert_knocked_out(&self) {
        for id in self.bricks {
            let b = &self.s.simulation().state().bricks[&id];
            assert!(
                !b.visible && !b.colliding && !b.raycast,
                "brick {id} still standing"
            );
            assert!(
                self.thrown.iter().any(|(t, _)| *t == id),
                "no debris for {id}"
            );
        }
    }
    fn assert_untouched(&self) {
        for id in self.bricks {
            assert!(self.standing(id), "brick {id} was knocked out");
        }
        assert!(self.thrown.is_empty(), "debris thrown: {:?}", self.thrown);
    }
}

fn fire(shot: Rocket) -> Blast {
    let mut s = with_weapons(shot.lan);
    let shooter = s
        .join(
            "Shooter".into(),
            Vec3::new(0.0, 0.05, 0.0),
            shot.drop_in_flight,
        )
        .unwrap();
    let builder = if shot.bystander_bricks {
        s.join("Bystander".into(), Vec3::new(6.0, 0.05, 0.0), false)
            .unwrap()
    } else {
        shooter
    };
    let bricks = [
        plant(&mut s, builder, 1, [0.0, 0.3, -8.0]),
        plant(&mut s, builder, 2, [1.0, 0.3, -8.0]),
    ];
    if let Some(settings) = shot.game {
        s.command(
            shooter,
            3,
            Command::MiniGame(MiniGameRequest::Create { color: 0, settings }),
        )
        .unwrap();
    }
    // The default minigame loadout carries the rocket launcher; outside a
    // minigame the shooter picks one up.
    let slot = match s.tool_inventories()[&shooter]
        .slots
        .iter()
        .position(|s| s.as_deref() == Some(ROCKET))
    {
        Some(slot) => slot,
        None => s.give_item(shooter, ROCKET).unwrap(),
    };
    s.command(shooter, 4, Command::EquipTool { slot: Some(slot) })
        .unwrap();
    for _ in 0..120 {
        s.step().unwrap();
    }
    s.take_cues();
    let aim = aim_at(&mut s, shooter, Vec3::new(0.0, 0.3, -8.0));
    s.command_with_aim(shooter, 5, Command::WeaponTrigger { down: true }, Some(aim))
        .unwrap();
    s.command_with_aim(
        shooter,
        6,
        Command::WeaponTrigger { down: false },
        Some(aim),
    )
    .unwrap();
    let mut thrown = Vec::new();
    s.step().unwrap();
    thrown.extend(kills(&mut s).into_iter().map(|(b, _, _)| (b, 1)));
    if shot.drop_in_flight {
        assert!(thrown.is_empty(), "the rocket landed before the drop");
        s.command(shooter, 7, Command::DropPlayerAtCamera(None))
            .unwrap();
    }
    for _ in 0..240 {
        s.step().unwrap();
        let tick = s.simulation().state().tick;
        thrown.extend(kills(&mut s).into_iter().map(|(b, _, _)| (b, tick)));
    }
    Blast { s, bricks, thrown }
}

fn brick_damage(on: bool) -> Option<Settings> {
    Some(Settings {
        brick_damage: on,
        ..Settings::default()
    })
}

/// Step until every brick stands again; the tick they came back.
fn respawned(blast: &mut Blast, within_ticks: u64) -> Option<u64> {
    for _ in 0..within_ticks {
        blast.s.step().unwrap();
        if blast.bricks.iter().all(|id| blast.standing(*id)) {
            return Some(blast.s.simulation().state().tick);
        }
    }
    None
}

#[test]
#[ignore = "requires the converted native weapons pack"]
fn rocket_knocks_bricks_out_in_a_brick_damage_minigame_and_they_respawn() {
    for lan in [false, true] {
        let mut blast = fire(Rocket {
            lan,
            bystander_bricks: false,
            game: brick_damage(true),
            drop_in_flight: false,
        });
        blast.assert_knocked_out();
        // Saving now keeps the fake-dead bricks as they will respawn.
        let shooter = blast.s.names().into_keys().next().unwrap();
        let Reply::Saved(build) = blast
            .s
            .command(
                shooter,
                8,
                Command::SaveBuild {
                    events: true,
                    ownership: true,
                },
            )
            .unwrap()
        else {
            panic!("expected a saved build")
        };
        assert!(
            build
                .world
                .bricks
                .values()
                .all(|b| b.visible && b.colliding && b.raycast)
        );
        // Autosaves and the host's final world keep them the same way.
        assert!(
            blast
                .s
                .saved_world()
                .bricks
                .values()
                .all(|b| b.visible && b.colliding && b.raycast)
        );
        // The minigame's brick respawn time (30 s by default) brings them back.
        let thrown_at = blast.thrown[0].1;
        let back = respawned(&mut blast, 60 * HZ).expect("bricks never respawned");
        assert!(
            back - thrown_at >= 30 * HZ,
            "back after {} ticks",
            back - thrown_at
        );
    }
}

#[test]
#[ignore = "requires the converted native weapons pack"]
fn rocket_leaves_bricks_alone_in_a_minigame_with_brick_damage_off() {
    for lan in [false, true] {
        for bystander_bricks in [false, true] {
            fire(Rocket {
                lan,
                bystander_bricks,
                game: brick_damage(false),
                drop_in_flight: false,
            })
            .assert_untouched();
        }
    }
}

/// Outside minigames, single player and LAN hosts break anyone's bricks
/// (`onExplode` checks nothing else under `$Server::LAN`); internet servers
/// only the shooter's own. They come back after the server's brick respawn
/// time, 30 s by default (`$Pref::Server::BrickRespawnTime`).
#[test]
#[ignore = "requires the converted native weapons pack"]
fn rocket_outside_a_minigame_follows_v20_lan_and_ownership_rules() {
    for (lan, bystander_bricks, breaks) in [
        (true, false, true),
        (true, true, true),
        (false, false, true),
        (false, true, false),
    ] {
        let mut blast = fire(Rocket {
            lan,
            bystander_bricks,
            game: None,
            drop_in_flight: false,
        });
        if !breaks {
            blast.assert_untouched();
            continue;
        }
        blast.assert_knocked_out();
        let thrown_at = blast.thrown[0].1;
        let back = respawned(&mut blast, 60 * HZ).expect("bricks never respawned");
        let after = back - thrown_at;
        assert!(
            (30 * HZ..31 * HZ).contains(&after),
            "lan {lan}, bystander {bystander_bricks}: back after {after} ticks"
        );
    }
}

#[test]
#[ignore = "requires the converted native weapons pack"]
fn lan_hosts_let_minigame_rockets_break_anyones_bricks_like_v20() {
    fire(Rocket {
        lan: true,
        bystander_bricks: true,
        game: brick_damage(true),
        drop_in_flight: false,
    })
    .assert_knocked_out();
    // Internet servers keep miniGameCanDamage's ownership rule.
    fire(Rocket {
        lan: false,
        bystander_bricks: true,
        game: brick_damage(true),
        drop_in_flight: false,
    })
    .assert_untouched();
}

/// `ProjectileData::onExplode` returns for 3 s after the shooter's F8 drop
/// inside a minigame, so a rocket already in flight breaks nothing.
#[test]
#[ignore = "requires the converted native weapons pack"]
fn a_rocket_in_flight_breaks_nothing_after_its_shooter_drops_in_a_minigame() {
    fire(Rocket {
        lan: true,
        bystander_bricks: false,
        game: brick_damage(true),
        drop_in_flight: true,
    })
    .assert_untouched();
}

/// The hammer deletes bricks for real (`killBrick`), inside a minigame
/// with brick damage off too: it asks trust, never the minigame.
#[test]
#[ignore = "requires the converted native weapons pack"]
fn hammer_deletes_bricks_for_good_whatever_the_minigame_says() {
    for game in [None, brick_damage(false)] {
        let mut s = with_weapons(true);
        let owner = s
            .join("Builder".into(), Vec3::new(0.0, 0.05, 0.0), false)
            .unwrap();
        let id = plant(&mut s, owner, 1, [0.0, 0.3, -2.5]);
        if let Some(settings) = game.clone() {
            s.command(
                owner,
                2,
                Command::MiniGame(MiniGameRequest::Create { color: 0, settings }),
            )
            .unwrap();
        }
        let hammer = s.tool_inventories()[&owner]
            .slots
            .iter()
            .position(|s| s.as_deref() == Some(HAMMER))
            .expect("hammer in the loadout");
        for _ in 0..30 {
            s.step().unwrap();
        }
        s.take_cues();
        aim_at(&mut s, owner, Vec3::new(0.0, 0.3, -2.5));
        swing(&mut s, owner, 3, hammer).unwrap();
        assert!(
            !s.simulation().state().bricks.contains_key(&id),
            "{game:?}: the hammer left the brick"
        );
        let kills = kills(&mut s);
        assert_eq!(kills.len(), 1);
        let (brick, origin, force) = kills[0];
        assert_eq!(brick, id);
        assert!(origin[1] < 0.3 && force > 0.0, "{origin:?} {force}");
        // Nothing brings it back.
        for _ in 0..(40 * HZ) {
            s.step().unwrap();
        }
        assert!(!s.simulation().state().bricks.contains_key(&id));
    }
}

/// `fakeKillBrick` is an event output: it needs no minigame and ignores
/// brick damage, and the brick returns after its own time (0 to 300 s).
#[test]
#[ignore = "requires the converted native event catalog"]
fn fake_kill_brick_ignores_brick_damage_and_respawns_on_its_own_time() {
    for game in [None, brick_damage(false)] {
        let mut s = with_weapons(false);
        let catalog = bri_events::Catalog::load(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../content/events-pack-002/catalog.json"),
        )
        .unwrap();
        s.set_event_catalog(catalog, Vec::new()).unwrap();
        let owner = s
            .join("Builder".into(), Vec3::new(0.0, 0.05, 0.0), false)
            .unwrap();
        let id = plant(&mut s, owner, 1, [0.0, 0.3, -4.0]);
        if let Some(settings) = game.clone() {
            s.command(
                owner,
                2,
                Command::MiniGame(MiniGameRequest::Create { color: 0, settings }),
            )
            .unwrap();
        }
        s.edit_brick(
            owner,
            id,
            Edit::Events(vec![EventRow {
                preserved: None,
                enabled: true,
                input: "onActivate".into(),
                delay_ms: 0,
                target: EventTarget::Slot(bri_events::Slot::SelfBrick),
                output: "fakeKillBrick".into(),
                params: vec![
                    EventValue::Vector(Vec3::new(0.0, 0.0, 10.0)),
                    EventValue::Int(5),
                ],
            }]),
        )
        .unwrap();
        s.take_cues();
        s.fire_brick_input(id, "onActivate", Some(owner));
        s.step().unwrap();
        let thrown = kills(&mut s);
        assert_eq!(thrown.len(), 1, "{game:?}");
        let (brick, _, force) = thrown[0];
        assert_eq!(brick, id);
        assert_eq!(force, 20.0, "VectorLen(vector) * 2");
        let b = &s.simulation().state().bricks[&id];
        assert!(!b.visible && !b.colliding && !b.raycast);
        for _ in 0..(4 * HZ) {
            s.step().unwrap();
        }
        assert!(
            !s.simulation().state().bricks[&id].visible,
            "back too early"
        );
        for _ in 0..(2 * HZ) {
            s.step().unwrap();
        }
        assert!(
            s.simulation().state().bricks[&id].visible,
            "never came back"
        );
    }
}

/// Planting passes the same build gate as painting and the wand: a mini-game
/// with building off refuses it, and leaving the mini-game allows it again.
#[test]
fn planting_obeys_the_minigame_building_rule() {
    let mut s = session();
    let builder = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    plant(&mut s, builder, 1, [0.0, 0.3, -4.0]);
    let settings = Settings {
        enable_building: false,
        ..Settings::default()
    };
    s.command(
        builder,
        2,
        Command::MiniGame(MiniGameRequest::Create { color: 0, settings }),
    )
    .unwrap();
    let refused = s
        .command(
            builder,
            3,
            Command::Plant {
                definition: "brick".into(),
                position: [2.0, 0.3, -4.0],
                quarter_turns: 0,
                color: 1,
            },
        )
        .unwrap_err();
    assert!(
        format!("{refused:#}").contains("Building is disabled"),
        "{refused:#}"
    );
    s.command(builder, 4, Command::MiniGame(MiniGameRequest::Leave))
        .unwrap();
    plant(&mut s, builder, 5, [2.0, 0.3, -4.0]);
}

const SYNTHETIC_ROCKET: &str = "test:weapon/rocketitem";
const SYNTHETIC_IMAGE: &str = "test:image/rocketimage";
const SYNTHETIC_PROJECTILE: &str = "test:projectile/rocket";

/// A content-free rocket launcher whose blast reaches 30 units.
fn synthetic_rocket_pack() -> bri_weapons::Pack {
    let state = |name: &str, ticks, script: &str| bri_weapons::State {
        name: name.into(),
        ticks,
        wait: true,
        allow_change: true,
        script: script.into(),
        ..Default::default()
    };
    let states = vec![
        bri_weapons::State {
            timeout: Some(1),
            ..state("Activate", 0, "")
        },
        bri_weapons::State {
            down: Some(2),
            ..state("Ready", 0, "")
        },
        bri_weapons::State {
            timeout: Some(3),
            ..state("Fire", 60, "onFire")
        },
        bri_weapons::State {
            timeout: Some(1),
            ..state("Reload", 0, "")
        },
    ];
    let image = bri_weapons::Image {
        id: SYNTHETIC_IMAGE.into(),
        name: "rocketLauncherImage".into(),
        model: String::new(),
        projectile: Some(SYNTHETIC_PROJECTILE.into()),
        mount_point: 0,
        offset: [0.; 3],
        eye_offset: [0.; 3],
        source_rotation_degrees: [0.; 3],
        correct_muzzle: false,
        melee: false,
        color: [1.; 4],
        color_shift: false,
        arm_ready: true,
        casing: String::new(),
        min_shot_ticks: 0,
        command: Default::default(),
        commands: Default::default(),
        shot: None,
        eye_rotation: [0.0; 3],
        zoom: None,
        crosshair: true,
        follow_arm: false,
        hide_nodes: Vec::new(),
        both_arms: false,
        paint_tint: false,
        left_image: None,
        magazine: None,
        volleys: vec![],
        last_shot: None,
        state_shots: Default::default(),
        cook: None,
        scripts: Default::default(),
        states,
    };
    let item = bri_weapons::Item {
        id: SYNTHETIC_ROCKET.into(),
        name: "rocketLauncherItem".into(),
        ui_name: "Rocket L.".into(),
        image: SYNTHETIC_IMAGE.into(),
        ..Default::default()
    };
    let projectile = bri_weapons::ProjectileDef {
        id: SYNTHETIC_PROJECTILE.into(),
        name: "rocketLauncherProjectile".into(),
        model: String::new(),
        speed: 40.,
        inherit: 0.,
        gravity: 0.,
        lifetime_ticks: 480,
        fade_ticks: 0,
        arm_ticks: 0,
        ballistic: false,
        elasticity: 0.,
        friction: 0.,
        damage: 0.,
        damage_type: String::new(),
        radius_damage_type: String::new(),
        impulse: 0.,
        vertical: 0.,
        explode_player: true,
        explode_death: true,
        collide_players: true,
        explosion: bri_weapons::Explosion {
            effect: String::new(),
            damage: 0.,
            radius: 10.,
            impulse: 0.,
            impulse_radius: 0.,
            impulse_vertical: 0.,
            burn_seconds: 0.,
        },
        brick: bri_weapons::BrickImpact {
            radius: 30.,
            direct: true,
            force: 20.,
            max_volume: 1000.,
            max_floating_volume: 1000.,
        },
        bounce_effect: String::new(),
        stick_effect: String::new(),
        blood_effect: String::new(),
        bounce_angle: 0.,
        min_stick_speed: 0.,
        trail: String::new(),
        sound: String::new(),
        light_radius: 0.,
        light_color: [0.; 3],
        sport_image: None,
        rest_speed: 0.,
        max_bounces: 0,
        children: Vec::new(),
        aura: None,
        slow: None,
        fixed_damage: false,
    };
    let pack = bri_weapons::Pack {
        effects: Default::default(),
        schema_version: bri_weapons::SCHEMA,
        id: "test.rockets".into(),
        items: [(SYNTHETIC_ROCKET.to_string(), item)].into(),
        images: [(SYNTHETIC_IMAGE.to_string(), image)].into(),
        projectiles: [(SYNTHETIC_PROJECTILE.to_string(), projectile)].into(),
        external_projectiles: Default::default(),
        damage_types: Default::default(),
        explosions: Default::default(),
        sounds: Default::default(),
        definitions: vec![],
        resources: vec![],
        diagnostics: vec![],
    };
    pack.validate().unwrap();
    pack
}

/// Fire one synthetic rocket whose blast reaches `brick_radius` into 160
/// bricks; how many it knocks out.
fn rocket_into_160_bricks(brick_radius: f32) -> usize {
    let (s, bricks) = rocket_into_160(brick_radius);
    bricks
        .iter()
        .filter(|id| !s.simulation().state().bricks[*id].colliding)
        .count()
}

/// The session after one synthetic rocket whose blast reaches
/// `brick_radius` hit 160 bricks, and those bricks.
fn rocket_into_160(brick_radius: f32) -> (Session, Vec<u64>) {
    let mut s = session();
    s.set_lan_host(true);
    let mut pack = synthetic_rocket_pack();
    pack.projectiles
        .get_mut(SYNTHETIC_PROJECTILE)
        .unwrap()
        .brick
        .radius = brick_radius;
    s.set_weapon_pack(pack).unwrap();
    let shooter = s
        .join("Shooter".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    let mut seq = 0;
    let mut bricks = Vec::new();
    for row in 0..16 {
        for column in 0..10 {
            seq += 1;
            let position = [column as f32 - 4.5, 0.3, -8.0 - row as f32];
            bricks.push(plant(&mut s, shooter, seq, position));
            // Stay under the host's action rate.
            for _ in 0..12 {
                s.step().unwrap();
            }
        }
    }
    let slot = s.give_item(shooter, SYNTHETIC_ROCKET).unwrap();
    seq += 1;
    s.command(shooter, seq, Command::EquipTool { slot: Some(slot) })
        .unwrap();
    for _ in 0..120 {
        s.step().unwrap();
    }
    let aim = aim_at(&mut s, shooter, Vec3::new(0.5, 0.3, -8.0));
    for down in [true, false] {
        seq += 1;
        s.command_with_aim(shooter, seq, Command::WeaponTrigger { down }, Some(aim))
            .unwrap();
    }
    for _ in 0..120 {
        s.step().unwrap();
    }
    (s, bricks)
}

/// v20's `onExplode` knocks out every eligible brick in the radius, with no
/// cap (it only sends them in messages of 100).
#[test]
fn a_rocket_knocks_out_every_brick_in_its_blast() {
    assert_eq!(rocket_into_160_bricks(30.0), 160);
}

/// A blast's bricks come back together in one tick with one collision
/// refresh, not one chunk rebuild and physics pass per brick (which stalled
/// the host for tens of milliseconds on a big build).
#[test]
fn a_blasts_bricks_respawn_together_with_one_collision_refresh() {
    let (mut s, bricks) = rocket_into_160(30.0);
    assert!(
        bricks
            .iter()
            .all(|id| !s.simulation().state().bricks[id].colliding)
    );
    for _ in 0..60 * HZ {
        let before = s.simulation().collision_refreshes();
        s.step().unwrap();
        let back = bricks
            .iter()
            .filter(|id| s.simulation().state().bricks[*id].colliding)
            .count();
        if back == 0 {
            continue;
        }
        assert_eq!(back, bricks.len(), "every brick respawns in the same tick");
        let refreshes = s.simulation().collision_refreshes() - before;
        assert!(
            refreshes <= 2,
            "{refreshes} collision refreshes in the respawn tick"
        );
        return;
    }
    panic!("bricks never respawned");
}

/// v20's `onCollision` knocks out only the brick a projectile hits; the
/// radius is `onExplode`'s.
#[test]
fn a_direct_hit_knocks_out_only_the_brick_it_hits() {
    assert_eq!(rocket_into_160_bricks(0.0), 1);
}

/// An internet host loads four bricks from a save whose builder is not on
/// the server (as in any old v20 save), then `shooter` makes a Brick Damage
/// minigame and fires one synthetic rocket at them; how many are knocked out.
fn rocket_into_loaded_save(ownership: bool, admin_shooter: bool) -> usize {
    let mut s = session();
    s.set_lan_host(false);
    s.set_weapon_pack(synthetic_rocket_pack()).unwrap();
    let host = s
        .join("Host".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    let shooter = if admin_shooter {
        host
    } else {
        s.join("Guest".into(), Vec3::new(0.0, 0.05, 0.0), false)
            .unwrap()
    };
    let mut saved = World::new("Arena".into(), "test".into(), vec![[1.0; 4]; 2]);
    for (i, x) in [-1.5f32, -0.5, 0.5, 1.5].into_iter().enumerate() {
        let mut brick = bri_world::Brick::new(
            ContentRef::Resolved("brick".into()),
            [x, 0.3, -8.0],
            // A v20 BL_ID with no principal behind it.
            4321,
        );
        brick.color = 1;
        saved.bricks.insert(i as u64 + 1, brick);
    }
    saved.next_brick_id = 5;
    let build = bri_world::build::SavedBuild::new(saved);
    s.command(
        host,
        1,
        Command::LoadBuild {
            build: Box::new(build),
            ownership,
        },
    )
    .unwrap();
    while s.build_loading() {
        s.step().unwrap();
    }
    let bricks: Vec<u64> = s.simulation().state().bricks.keys().copied().collect();
    assert_eq!(bricks.len(), 4);
    s.command(
        shooter,
        2,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Settings {
                loadout: Default::default(),
                ..Settings::default()
            },
        }),
    )
    .unwrap();
    let slot = s.give_item(shooter, SYNTHETIC_ROCKET).unwrap();
    s.command(shooter, 3, Command::EquipTool { slot: Some(slot) })
        .unwrap();
    for _ in 0..120 {
        s.step().unwrap();
    }
    let aim = aim_at(&mut s, shooter, Vec3::new(0.0, 0.3, -8.0));
    for (seq, down) in [(4, true), (5, false)] {
        s.command_with_aim(shooter, seq, Command::WeaponTrigger { down }, Some(aim))
            .unwrap();
    }
    for _ in 0..120 {
        s.step().unwrap();
    }
    bricks
        .iter()
        .filter(|id| !s.simulation().state().bricks[*id].colliding)
        .count()
}

/// Max's report: a host who can paint and hammer a save loaded with
/// ownership found their Brick Damage minigame could not break it, though
/// the same save loaded without ownership broke. Bricks whose builder is
/// away count as the minigame owner's when that owner has Full trust over
/// them; a player without that trust still cannot break them.
#[test]
fn a_brick_damage_minigame_breaks_a_save_its_owner_may_hammer() {
    assert_eq!(rocket_into_loaded_save(false, true), 4);
    assert_eq!(rocket_into_loaded_save(true, true), 4);
    assert_eq!(rocket_into_loaded_save(true, false), 0);
}
