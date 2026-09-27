//! Brick damage: rockets knock bricks out in a brick-damage minigame, and
//! every brick death is announced for client debris.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_minigames::Settings;
use bri_sim::{
    definitions::{Definition, Definitions},
    presentation::CueKind,
    session::{ActionAim, Command, MiniGameRequest, Reply, Session},
    simulation::Simulation,
};
use bri_world::{ContentRef, World};
use glam::Vec3;
use rapier3d::prelude::*;

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

// The hammer now fires through the weapon runtime; rewrite with a weapon trigger.
#[cfg(any())]
#[test]
fn hammer_kills_a_brick_and_throws_it_up_off_its_spot() {
    let mut s = session();
    let owner = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    let id = plant(&mut s, owner, 1, [0.5, 0.3, -3.5]);
    for _ in 0..30 {
        s.step().unwrap();
    }
    s.take_cues();
    s.equip_tool(owner, Some(0)).unwrap();
    let eye = s.snapshot().players[0].eye(&Default::default());
    let d = Vec3::new(0.5, 0.3, -3.5) - eye;
    let aim = ActionAim {
        yaw: d.x.atan2(-d.z),
        pitch: d.y.atan2(Vec3::new(d.x, 0.0, d.z).length()),
    };
    s.command_with_aim(
        owner,
        2,
        Command::Tool(bri_sim::session::ToolAction::Hammer),
        Some(aim),
    )
    .unwrap();
    assert!(!s.simulation().state().bricks.contains_key(&id));
    let kills = kills(&mut s);
    assert_eq!(kills.len(), 1);
    let (brick, origin, force) = kills[0];
    assert_eq!(brick, id);
    assert!(origin[1] < 0.3 && force > 0.0, "{origin:?} {force}");
}

/// A shooter in a brick-damage minigame fires one rocket at two bricks
/// planted by `builder` (the shooter, or a bystander outside the minigame).
/// Returns the session and the bricks' ids after the blast.
fn rocket_at_bricks(lan: bool, bystander_bricks: bool) -> (Session, [u64; 2], Vec<u64>) {
    let pack = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/weapons-pack-003/weapons.json");
    let mut s = session();
    s.set_lan_host(lan);
    s.set_weapon_pack(bri_weapons::Pack::from_json(&std::fs::read(pack).unwrap()).unwrap())
        .unwrap();
    let shooter = s
        .join("Shooter".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    let builder = if bystander_bricks {
        s.join("Bystander".into(), Vec3::new(6.0, 0.05, 0.0), false)
            .unwrap()
    } else {
        shooter
    };
    let bricks = [
        plant(&mut s, builder, 1, [0.0, 0.3, -8.0]),
        plant(&mut s, builder, 2, [1.0, 0.3, -8.0]),
    ];
    let settings = Settings {
        brick_damage: true,
        ..Settings::default()
    };
    s.command(
        shooter,
        3,
        Command::MiniGame(MiniGameRequest::Create { color: 0, settings }),
    )
    .unwrap();
    // The default minigame loadout carries the rocket launcher.
    let slot = s.tool_inventories()[&shooter]
        .slots
        .iter()
        .position(|s| s.as_deref() == Some("v20.weapon.rocketlauncheritem"))
        .expect("rocket launcher in the minigame loadout");
    s.command(shooter, 4, Command::EquipTool { slot: Some(slot) })
        .unwrap();
    for _ in 0..120 {
        s.step().unwrap();
    }
    s.take_cues();
    let eye = s.snapshot().players[0].eye(&Default::default());
    let d = Vec3::new(0.0, 0.3, -8.0) - eye;
    let aim = ActionAim {
        yaw: d.x.atan2(-d.z),
        pitch: d.y.atan2(Vec3::new(d.x, 0.0, d.z).length()),
    };
    // A live client streams movement; idle shooters have their triggers dropped.
    s.movement(
        shooter,
        1,
        bri_sim::player::MoveInput {
            yaw: aim.yaw,
            pitch: aim.pitch,
            ..Default::default()
        },
    )
    .unwrap();
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
    for _ in 0..240 {
        s.step().unwrap();
        thrown.extend(kills(&mut s).into_iter().map(|(brick, _, _)| brick));
    }
    (s, bricks, thrown)
}

#[test]
#[ignore = "requires the converted native weapons pack"]
fn rocket_knocks_bricks_out_in_a_brick_damage_minigame_and_they_respawn() {
    for lan in [false, true] {
        let (mut s, bricks, thrown) = rocket_at_bricks(lan, false);
        for id in bricks {
            let b = &s.simulation().state().bricks[&id];
            assert!(
                !b.visible && !b.colliding && !b.raycast,
                "brick {id} still standing"
            );
            assert!(thrown.contains(&id), "no debris for {id}");
        }
        // The minigame's brick respawn time brings them back.
        let mut back = false;
        for _ in 0..(120 * 60) {
            s.step().unwrap();
            if bricks
                .iter()
                .all(|id| s.simulation().state().bricks[id].visible)
            {
                back = true;
                break;
            }
        }
        assert!(back, "bricks never respawned");
    }
}

#[test]
#[ignore = "requires the converted native weapons pack"]
fn lan_hosts_let_minigame_rockets_break_anyones_bricks_like_v20() {
    let (s, bricks, thrown) = rocket_at_bricks(true, true);
    for id in bricks {
        assert!(
            !s.simulation().state().bricks[&id].visible,
            "brick {id} still standing"
        );
        assert!(thrown.contains(&id));
    }
    // Internet servers keep miniGameCanDamage's ownership rule.
    let (s, bricks, thrown) = rocket_at_bricks(false, true);
    for id in bricks {
        assert!(s.simulation().state().bricks[&id].visible);
    }
    assert!(thrown.is_empty());
}
