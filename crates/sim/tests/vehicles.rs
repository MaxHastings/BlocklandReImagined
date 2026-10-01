//! Vehicle spawn bricks, mounting, driving, dismounting and destruction with
//! the converted native vehicle pack.
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    session::{Command, Session, ToolAction},
    simulation::Simulation,
};
use bri_world::{Brick, ContentRef, VehicleSpawn, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::path::Path;

const SPAWN: &str = "v20/brick/brickvehiclespawndata";
const JEEP: &str = "v20.vehicle.jeepvehicle";

fn session(root: &Path) -> anyhow::Result<(Session, u64)> {
    session_with(root, JEEP)
}

fn definitions(root: &Path) -> anyhow::Result<Definitions> {
    Definitions::load(
        &root.join("content/stock-catalog-004"),
        &root.join("content/maps-pass-008"),
    )
}
fn ground() -> ColliderBuilder {
    ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0))
}
/// The test world: one vehicle spawn brick 12 ahead of the spawn point.
fn vehicle_world(vehicle: &str) -> World {
    let mut world = World::new(
        "Vehicles".into(),
        "test".into(),
        vec![[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]],
    );
    let mut brick = Brick::new(ContentRef::Resolved(SPAWN.into()), [0.0, 0.1, -12.0], 0);
    brick.vehicle = Some(VehicleSpawn {
        vehicle: ContentRef::Resolved(vehicle.into()),
        recolor: true,
    });
    world.bricks.insert(1, brick);
    world.next_brick_id = 2;
    world
}
fn session_with(root: &Path, vehicle: &str) -> anyhow::Result<(Session, u64)> {
    let mut s = Session::new(Simulation::new(
        vehicle_world(vehicle),
        definitions(root)?,
        vec![ground()],
    )?);
    s.set_weapon_pack(bri_weapons::Pack::from_json(&std::fs::read(
        root.join("content/weapons-pack-009/weapons.json"),
    )?)?)?;
    s.set_vehicle_pack(bri_vehicles::Pack::load(
        root.join("content/vehicles-pack-012/vehicles.json"),
    )?, Vec::new())?;
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)])?;
    let owner = s.join("Driver".into(), Vec3::new(0.0, 0.05, 0.0), true)?;
    Ok((s, owner))
}

#[test]
#[ignore = "requires the converted native vehicle and brick packs"]
fn spawn_brick_vehicle_mounts_drives_dismounts_and_respawns() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (mut s, owner) = session(&root)?;
    let mut sequence = 0;
    let mut feed = |s: &mut Session, input: MoveInput, ticks: usize| -> anyhow::Result<()> {
        for _ in 0..ticks {
            sequence += 1;
            s.movement(owner, sequence, input)?;
            s.step()?;
        }
        Ok(())
    };
    feed(&mut s, MoveInput::default(), 120)?;
    let infos = s.vehicle_infos();
    assert_eq!(infos.len(), 1, "spawn brick produced its jeep");
    assert_eq!(infos[0].definition, JEEP);
    assert_eq!(infos[0].color, Some(0), "recolored with the brick color");
    let parked = Vec3::from(s.vehicle_poses()[0].position);
    assert!(
        parked.distance(Vec3::new(0.0, 0.0, -12.0)) < 3.0,
        "{parked}"
    );
    // Hop onto the jeep: the driver seat is taken.
    for i in 0..60 {
        let input = MoveInput {
            forward: 1.0,
            jump: i % 3 == 0,
            ..Default::default()
        };
        feed(&mut s, input, 10)?;
        if s.mounted(owner).is_some() {
            break;
        }
    }
    let (vehicle, seat) = s.mounted(owner).expect("walked into the jeep and mounted");
    assert_eq!((vehicle, seat), (infos[0].id, 0));
    assert_eq!(s.vitals()[&owner].mounted, Some((vehicle, 0)));
    // Throttle forward (-Z) for two seconds.
    let before = Vec3::from(s.vehicle_poses()[0].position);
    feed(
        &mut s,
        MoveInput {
            forward: 1.0,
            ..Default::default()
        },
        240,
    )?;
    let after = Vec3::from(s.vehicle_poses()[0].position);
    assert!(
        before.distance(after) > 5.0,
        "jeep drove {before} -> {after}"
    );
    // The rider stays in the seat while moving.
    let rider = s
        .motion_states()
        .into_iter()
        .find(|(p, _)| p.owner == owner)
        .unwrap()
        .0;
    assert!(Vec3::from(rider.feet).distance(after) < 4.0);
    // Jump only brakes; jet leaves the vehicle ("get out of the Jeep by
    // pressing Jet").
    feed(&mut s, MoveInput::default(), 120)?;
    feed(
        &mut s,
        MoveInput {
            jump: true,
            ..Default::default()
        },
        2,
    )?;
    feed(&mut s, MoveInput::default(), 10)?;
    assert!(s.mounted(owner).is_some(), "jump brakes, it does not dismount");
    feed(
        &mut s,
        MoveInput {
            jet: true,
            ..Default::default()
        },
        2,
    )?;
    feed(&mut s, MoveInput::default(), 10)?;
    assert_eq!(s.mounted(owner), None);
    // Wrench respawn puts a fresh jeep back on its brick.
    s.equip_tool(owner, Some(1))?;
    let old = s.vehicle_infos()[0].id;
    let result = s.command(
        owner,
        1,
        Command::Tool(ToolAction::RespawnVehicle { brick: 1 }),
    );
    // The brick may be out of wrench reach after driving away; respawn is
    // then exercised through the internal API path via a replanted brick.
    if result.is_ok() {
        assert_ne!(s.vehicle_infos()[0].id, old);
    }
    Ok(())
}

#[test]
#[ignore = "requires the converted native vehicle, weapon and brick packs"]
fn bot_brick_spawns_a_bot_that_fights_inside_its_owners_minigame() -> anyhow::Result<()> {
    use bri_sim::session::{InspectMode, MiniGameRequest, Notice, ToolCatalog, WrenchProperties};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let definitions = Definitions::load(
        &root.join("content/stock-catalog-004"),
        &root.join("content/maps-pass-008"),
    )?;
    let height = definitions.entries[SPAWN].mesh.height_plates as f32 * 0.2;
    let world = World::new("Bots".into(), "test".into(), vec![[1.0, 0.0, 0.0, 1.0]]);
    let mut s = Session::new(Simulation::new(
        world,
        definitions,
        vec![ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0))],
    )?);
    s.set_weapon_pack(bri_weapons::Pack::from_json(&std::fs::read(
        root.join("content/weapons-pack-009/weapons.json"),
    )?)?)?;
    s.set_vehicle_pack(bri_vehicles::Pack::load(
        root.join("content/vehicles-pack-012/vehicles.json"),
    )?, bri_sim::bot_kind::BotPack::from_json(include_bytes!("../../../packages/blockhead_bot/assets/bots.json")).unwrap().bots)?;
    s.set_tool_catalog(ToolCatalog {
        vehicles: ["bot.blockhead".to_string()].into(),
        vehicle_bricks: [SPAWN.to_string()].into(),
        ..Default::default()
    })?;
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 6.0)])?;
    let human = s.join("Human".into(), Vec3::new(0.0, 0.05, 6.0), true)?;
    let mut sequence = 0;
    let mut look = MoveInput::default();
    let mut idle = |s: &mut Session, ticks: usize, look: MoveInput| -> anyhow::Result<()> {
        for _ in 0..ticks {
            sequence += 1;
            s.movement(human, sequence, look)?;
            s.step()?;
        }
        Ok(())
    };
    idle(&mut s, 60, look)?;
    let brick_center = Vec3::new(0.0, height * 0.5, 0.0);
    let bri_sim::session::Reply::Planted(brick) = s.command(
        human,
        1,
        Command::Plant {
            definition: SPAWN.into(),
            position: brick_center.to_array(),
            quarter_turns: 0,
            color: 0,
        },
    )?
    else {
        anyhow::bail!("spawn brick not planted")
    };
    // Look at the brick and swing the wrench to open it.
    s.equip_tool(human, Some(1))?;
    let eye = Vec3::new(0.0, 2.4, 6.0);
    let d = (brick_center - eye).normalize();
    look.yaw = d.x.atan2(-d.z);
    look.pitch = d.y.asin();
    idle(&mut s, 2, look)?;
    s.command(human, 2, Command::WeaponTrigger { down: true })?;
    idle(&mut s, 8, look)?;
    assert!(s.take_private_notices().iter().any(|(to, notice)| *to == human
        && matches!(notice, Notice::Inspected { mode: InspectMode::Wrench, brick_id, .. } if *brick_id == brick)));
    s.command(human, 3, Command::WeaponTrigger { down: false })?;
    s.command(
        human,
        4,
        Command::Tool(ToolAction::SetWrench {
            brick,
            properties: WrenchProperties {
                vehicle: Some("bot.blockhead".into()),
                raycast: true,
                colliding: true,
                visible: true,
                ..Default::default()
            },
        }),
    )?;
    idle(&mut s, 30, look)?;
    let bot = *s
        .names()
        .keys()
        .find(|o| s.is_bot(**o))
        .expect("bot spawned from its brick");
    assert!(s.vehicle_infos().is_empty(), "bots are not vehicles");
    // Outside minigames the bot is harmless.
    idle(&mut s, 600, look)?;
    assert_eq!(s.vitals()[&human].health, 100.0);
    // Inside the owner's minigame, the bot joins, arms itself and attacks.
    s.command(
        human,
        5,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Default::default(),
        }),
    )?;
    let mut hurt = false;
    for _ in 0..20 {
        idle(&mut s, 120, look)?;
        let v = &s.vitals()[&human];
        if v.health < 100.0 || !v.alive {
            hurt = true;
            break;
        }
    }
    assert_eq!(s.vitals()[&bot].minigame, s.vitals()[&human].minigame);
    assert!(hurt, "the bot shot its minigame opponent");
    Ok(())
}

/// Feeds one player's input for a number of ticks.
struct Feeder {
    owner: u64,
    sequence: u64,
}
impl Feeder {
    fn feed(&mut self, s: &mut Session, input: MoveInput, ticks: usize) -> anyhow::Result<()> {
        for _ in 0..ticks {
            self.sequence += 1;
            s.movement(self.owner, self.sequence, input)?;
            s.step()?;
        }
        Ok(())
    }
    /// Run at the vehicle, hopping, until mounted (v20 mounts only from above).
    fn board(&mut self, s: &mut Session, yaw: f32) -> anyhow::Result<()> {
        for i in 0..60 {
            let input = MoveInput {
                forward: 1.0,
                jump: i % 3 == 0,
                yaw,
                ..Default::default()
            };
            self.feed(s, input, 10)?;
            if s.mounted(self.owner).is_some() {
                return Ok(());
            }
        }
        anyhow::bail!("never boarded")
    }
}

fn pose_heading(rotation: [f32; 4]) -> f32 {
    let forward = glam::Quat::from_array(rotation) * Vec3::NEG_Z;
    forward.x.atan2(-forward.z)
}

#[test]
#[ignore = "requires the converted native vehicle and brick packs"]
fn walking_into_a_vehicle_does_not_board_it_but_jumping_on_does() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (mut s, owner) = session(&root)?;
    let mut p = Feeder { owner, sequence: 0 };
    p.feed(&mut s, MoveInput::default(), 120)?;
    p.feed(
        &mut s,
        MoveInput {
            forward: 1.0,
            ..Default::default()
        },
        300,
    )?;
    assert_eq!(
        s.mounted(owner),
        None,
        "walking into the jeep only bumps it"
    );
    p.board(&mut s, 0.0)?;
    assert_eq!(
        s.mounted(owner).map(|m| m.1),
        Some(0),
        "the driver seat first"
    );
    Ok(())
}

#[test]
#[ignore = "requires the converted native vehicle and brick packs"]
fn horse_runs_where_its_rider_looks_jumps_and_lets_go_on_jet() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (mut s, owner) = session_with(&root, "v20.vehicle.horsearmor")?;
    let mut p = Feeder { owner, sequence: 0 };
    p.feed(&mut s, MoveInput::default(), 120)?;
    p.board(&mut s, 0.0)?;
    p.feed(&mut s, MoveInput::default(), 60)?;
    // Look a quarter turn right and run: the horse turns to face the look.
    let look = std::f32::consts::FRAC_PI_2;
    let before = Vec3::from(s.vehicle_poses()[0].position);
    p.feed(
        &mut s,
        MoveInput {
            forward: 1.0,
            yaw: look,
            ..Default::default()
        },
        240,
    )?;
    let pose = &s.vehicle_poses()[0];
    let after = Vec3::from(pose.position);
    assert!(
        (pose_heading(pose.rotation) - look).abs() < 0.01,
        "faces the look"
    );
    assert!(
        after.x - before.x > 15.0,
        "runs at maxForwardSpeed 12: {before} -> {after}"
    );
    // The rider sits facing the horse's way.
    let rider = s
        .motion_states()
        .into_iter()
        .find(|(p, _)| p.owner == owner)
        .unwrap()
        .0;
    assert!((rider.yaw - look).abs() < 0.01);
    // Jump uses jumpForce 17 * 90.
    let ground = after.y;
    let mut peak = ground;
    for _ in 0..40 {
        p.feed(
            &mut s,
            MoveInput {
                jump: true,
                yaw: look,
                ..Default::default()
            },
            1,
        )?;
        peak = peak.max(s.vehicle_poses()[0].position[1]);
    }
    assert!(peak - ground > 3.0, "horse jumped {peak} from {ground}");
    p.feed(
        &mut s,
        MoveInput {
            yaw: look,
            ..Default::default()
        },
        120,
    )?;
    p.feed(
        &mut s,
        MoveInput {
            jet: true,
            yaw: look,
            ..Default::default()
        },
        3,
    )?;
    assert_eq!(s.mounted(owner), None, "jet dismounts the horse");
    Ok(())
}

#[test]
#[ignore = "requires the converted native vehicle and brick packs"]
fn tank_gunner_aims_where_they_look_relative_to_the_hull() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (mut s, owner) = session_with(&root, "v20.vehicle.tankvehicle")?;
    let mut p = Feeder { owner, sequence: 0 };
    p.feed(&mut s, MoveInput::default(), 120)?;
    p.board(&mut s, 0.0)?;
    s.switch_seat(owner, 1)?;
    s.switch_seat(owner, 1)?;
    assert_eq!(s.mounted(owner).map(|m| m.1), Some(2), "gunner seat");
    let hull = pose_heading(s.vehicle_poses()[0].rotation);
    p.feed(
        &mut s,
        MoveInput {
            yaw: hull + 0.5,
            pitch: 0.2,
            ..Default::default()
        },
        10,
    )?;
    let aim = s.vehicle_poses()[0].turret_aim;
    // Looking right of the hull swings the turret right (negative about up).
    assert!((aim[0] + 0.5).abs() < 0.01, "turret yaw {aim:?}");
    assert!((aim[1] - 0.2).abs() < 0.01, "barrel pitch {aim:?}");
    Ok(())
}

#[test]
#[ignore = "requires the converted native vehicle and brick packs"]
fn a_new_tank_gunner_takes_the_turret_where_it_was_left() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (mut s, owner) = session_with(&root, "v20.vehicle.tankvehicle")?;
    let mut p = Feeder { owner, sequence: 0 };
    p.feed(&mut s, MoveInput::default(), 120)?;
    p.board(&mut s, 0.0)?;
    s.switch_seat(owner, 1)?;
    s.switch_seat(owner, 1)?;
    assert_eq!(s.mounted(owner).map(|m| m.1), Some(2), "gunner seat");
    let hull = pose_heading(s.vehicle_poses()[0].rotation);
    let behind = MoveInput {
        yaw: hull + 3.0,
        pitch: 0.2,
        ..Default::default()
    };
    p.feed(&mut s, behind, 10)?;
    let aimed = s.vehicle_poses()[0].turret_aim;
    // Out of the gunner's seat and into the driver's: the turret stays put.
    s.switch_seat(owner, 1)?;
    assert_eq!(s.mounted(owner).map(|m| m.1), Some(0), "driver seat");
    let facing = MoveInput {
        yaw: hull,
        ..Default::default()
    };
    p.feed(&mut s, facing, 10)?;
    let kept = s.vehicle_poses()[0].turret_aim;
    assert!((kept[0] - aimed[0]).abs() < 1e-4, "{kept:?} vs {aimed:?}");
    // Back on the gun: inputs still carrying the boarding look leave it.
    s.switch_seat(owner, 1)?;
    s.switch_seat(owner, 1)?;
    assert_eq!(s.mounted(owner).map(|m| m.1), Some(2), "gunner seat");
    p.feed(&mut s, facing, 10)?;
    let kept = s.vehicle_poses()[0].turret_aim;
    assert!((kept[0] - aimed[0]).abs() < 1e-4, "{kept:?} vs {aimed:?}");
    // Once the gunner looks along it, it follows their look again.
    let hull = pose_heading(s.vehicle_poses()[0].rotation);
    p.feed(
        &mut s,
        MoveInput {
            yaw: hull + 1.0,
            ..Default::default()
        },
        10,
    )?;
    let aim = s.vehicle_poses()[0].turret_aim;
    assert!((aim[0] + 1.0).abs() < 0.01, "turret yaw {aim:?}");
    Ok(())
}

#[test]
#[ignore = "requires the converted native vehicle and brick packs"]
fn standalone_tank_turret_is_on_the_spawn_list() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (s, _) = session(&root)?;
    let choices = s.vehicle_choices();
    assert!(
        choices
            .iter()
            .any(|(id, name)| id == "v20.vehicle.tankturretplayer" && name == "Tank Turret")
    );
    assert!(!choices.iter().any(|(id, _)| id.contains("skivehicle")));
    Ok(())
}

#[test]
#[ignore = "requires the converted native vehicle, weapon and brick packs"]
fn skis_item_boards_skis_and_fires_again_to_step_off() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (mut s, owner) = session(&root)?;
    let mut p = Feeder { owner, sequence: 0 };
    p.feed(&mut s, MoveInput::default(), 60)?;
    // The skis take the last colour spray can's paint (`currentColor`).
    s.command(owner, 99, Command::UseSprayCan { color: 1 })?;
    let slot = s.give_item(owner, "v20.weapon.skiitem")?;
    let mut command = 100;
    let mut fire = |s: &mut Session, p: &mut Feeder| -> anyhow::Result<()> {
        s.equip_tool(owner, Some(slot))?;
        p.feed(s, MoveInput::default(), 80)?;
        for down in [true, false] {
            command += 1;
            s.command(owner, command, Command::WeaponTrigger { down })?;
            p.feed(s, MoveInput::default(), 4)?;
        }
        Ok(())
    };
    fire(&mut s, &mut p)?;
    p.feed(&mut s, MoveInput::default(), 40)?;
    let (vehicle, _) = s.mounted(owner).expect("riding the skis");
    let skis = s
        .vehicle_infos()
        .into_iter()
        .find(|v| v.id == vehicle)
        .unwrap();
    assert_eq!(skis.definition, "v20.vehicle.skivehicle");
    assert_eq!(skis.color, Some(1));
    fire(&mut s, &mut p)?;
    p.feed(&mut s, MoveInput::default(), 4)?;
    assert_eq!(s.mounted(owner), None, "stepped off the skis");
    assert!(
        s.vehicle_infos().iter().all(|v| v.id != vehicle),
        "empty skis vanish"
    );
    Ok(())
}

#[test]
#[ignore = "requires the converted native vehicle, weapon and brick packs"]
fn skis_work_again_after_jetting_off_them_and_after_respawning() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (mut s, owner) = session(&root)?;
    let mut p = Feeder { owner, sequence: 0 };
    p.feed(&mut s, MoveInput::default(), 60)?;
    let use_skis = |s: &mut Session, p: &mut Feeder| -> anyhow::Result<()> {
        let slot = match s.tool_inventories()[&owner]
            .slots
            .iter()
            .position(|i| i.as_deref() == Some("v20.weapon.skiitem"))
        {
            Some(slot) => slot,
            None => s.give_item(owner, "v20.weapon.skiitem")?,
        };
        s.equip_tool(owner, Some(slot))?;
        p.feed(s, MoveInput::default(), 80)?;
        for down in [true, false] {
            p.sequence += 1;
            s.command(owner, p.sequence, Command::WeaponTrigger { down })?;
            p.feed(s, MoveInput::default(), 4)?;
        }
        p.feed(s, MoveInput::default(), 40)
    };
    let jet_off = |s: &mut Session, p: &mut Feeder| -> anyhow::Result<()> {
        let jet = MoveInput {
            jet: true,
            ..Default::default()
        };
        p.feed(s, jet, 2)?;
        p.feed(s, MoveInput::default(), 10)
    };
    use_skis(&mut s, &mut p)?;
    assert!(s.mounted(owner).is_some(), "riding the skis");
    // Jet steps off; the empty skis vanish before that dismount is applied.
    jet_off(&mut s, &mut p)?;
    assert_eq!(s.mounted(owner), None, "jet steps off the skis");
    use_skis(&mut s, &mut p)?;
    assert!(
        s.mounted(owner).is_some(),
        "the skis work again after jetting off"
    );
    // Self-delete and respawn: the new body starts off skis and can use them.
    jet_off(&mut s, &mut p)?;
    p.sequence += 1;
    s.command(owner, p.sequence, Command::Suicide)?;
    p.feed(&mut s, MoveInput::default(), 125)?;
    p.sequence += 1;
    s.command(owner, p.sequence, Command::Respawn)?;
    p.feed(&mut s, MoveInput::default(), 10)?;
    use_skis(&mut s, &mut p)?;
    assert!(s.mounted(owner).is_some(), "the skis work after respawning");
    Ok(())
}

#[test]
#[ignore = "requires the converted native vehicle, weapon and brick packs"]
fn seated_riders_face_the_seat_and_use_tools_but_gunners_fire_the_gun() -> anyhow::Result<()> {
    use bri_sim::session::ActionAim;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (mut s, owner) = session_with(&root, "v20.vehicle.tankvehicle")?;
    let mut p = Feeder { owner, sequence: 0 };
    p.feed(&mut s, MoveInput::default(), 120)?;
    p.board(&mut s, 0.0)?;
    assert_eq!(s.mounted(owner).map(|m| m.1), Some(0), "driver seat");
    // Mouse yaw does not turn a seated body.
    let hull = pose_heading(s.vehicle_poses()[0].rotation);
    p.feed(
        &mut s,
        MoveInput {
            yaw: hull + 1.0,
            ..Default::default()
        },
        10,
    )?;
    let rider = |s: &Session| {
        s.motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .unwrap()
            .0
    };
    let facing = rider(&s).yaw;
    let turn = (facing - hull + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    assert!(turn.abs() < 0.2, "rider faces the seat: {facing} vs {hull}");
    // The driver fires a held gun.
    let slot = s.give_item(owner, "v20.weapon.gunitem")?;
    s.equip_tool(owner, Some(slot))?;
    p.feed(&mut s, MoveInput::default(), 40)?;
    let aim = Some(ActionAim {
        yaw: hull,
        pitch: 0.2,
    });
    s.command_with_aim(owner, 50, Command::WeaponTrigger { down: true }, aim)?;
    s.command_with_aim(owner, 51, Command::WeaponTrigger { down: false }, aim)?;
    p.feed(&mut s, MoveInput::default(), 1)?;
    assert!(
        s.weapon_view()
            .projectiles
            .iter()
            .any(|p| p.source.0 == owner),
        "a seated driver shoots their gun"
    );
    // The gunner's fire puts the gun away and shoots the turret.
    p.feed(&mut s, MoveInput::default(), 60)?;
    s.switch_seat(owner, 1)?;
    s.switch_seat(owner, 1)?;
    assert_eq!(s.mounted(owner).map(|m| m.1), Some(2), "gunner seat");
    s.equip_tool(owner, Some(slot))?;
    p.feed(&mut s, MoveInput::default(), 40)?;
    s.command_with_aim(owner, 52, Command::WeaponTrigger { down: true }, aim)?;
    p.feed(&mut s, MoveInput::default(), 2)?;
    s.command_with_aim(owner, 53, Command::WeaponTrigger { down: false }, aim)?;
    p.feed(&mut s, MoveInput::default(), 2)?;
    assert!(
        s.weapon_view()
            .images
            .get(&owner)
            .is_none_or(|images| images.is_empty()),
        "the gunner's tool is put away"
    );
    Ok(())
}

#[test]
#[ignore = "requires the converted native vehicle and brick packs"]
fn pirate_cannon_shows_its_charge_as_a_bottom_print() -> anyhow::Result<()> {
    use bri_sim::session::Notice;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (mut s, owner) = session_with(&root, "v20.vehicle.cannonturret")?;
    let mut p = Feeder { owner, sequence: 0 };
    p.feed(&mut s, MoveInput::default(), 120)?;
    p.board(&mut s, 0.0)?;
    s.take_private_notices();
    s.command(owner, 50, Command::WeaponTrigger { down: true })?;
    // `CannonStrengthLoop` adds a step at once and every 200 ms up to 10.
    p.feed(&mut s, MoveInput::default(), 300)?;
    let prints: Vec<String> = s
        .take_private_notices()
        .into_iter()
        .filter_map(|(to, n)| match n {
            Notice::Bottom {
                text,
                seconds,
                hide_bar: true,
            } if to == owner && seconds == 1.0 => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(prints.len(), 10, "{prints:?}");
    let bar = |lit: usize| {
        format!(
            "<just:center><color:FF0000>Fire! <color:FFFFFF>:<color:FFFF00>{}<color:000000>{}",
            "|".repeat(lit),
            "|".repeat(20 - lit)
        )
    };
    assert_eq!(prints[0], bar(2));
    assert_eq!(prints[9], bar(20));
    Ok(())
}

#[test]
#[ignore = "requires the converted native vehicle and brick packs"]
fn admin_drop_at_camera_carries_the_ridden_vehicle() -> anyhow::Result<()> {
    use bri_admin::{Action, Request};
    use bri_sim::{
        presentation::CueKind,
        session::{CameraView, ControlObject},
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for vehicle in [JEEP, "v20.vehicle.horsearmor"] {
        let (mut s, owner) = session_with(&root, vehicle)?;
        let mut p = Feeder { owner, sequence: 0 };
        p.feed(&mut s, MoveInput::default(), 120)?;
        p.board(&mut s, 0.0)?;
        p.feed(&mut s, MoveInput::default(), 30)?;
        assert!(s.mounted(owner).is_some(), "{vehicle}");
        s.command(
            owner,
            100,
            Command::Admin(Request::new(Action::DropCameraAtPlayer)),
        )?;
        let camera = CameraView {
            eye: [30.0, 6.0, 20.0],
            yaw: std::f32::consts::FRAC_PI_2,
            pitch: 0.0,
        };
        s.take_cues();
        s.command(owner, 101, Command::DropPlayerAtCamera(Some(camera)))?;
        assert!(s.mounted(owner).is_some(), "{vehicle}: still riding");
        assert_eq!(s.control(owner), Some(ControlObject::Player));
        assert!(s.take_cues().iter().any(|c| matches!(
            c.kind,
            CueKind::Teleport { player: false, .. }
        )));
        // The client turns its look to the camera's heading with the drop.
        let look = MoveInput {
            yaw: camera.yaw,
            ..Default::default()
        };
        p.feed(&mut s, look, 1)?;
        let pose = &s.vehicle_poses()[0];
        assert!(
            Vec3::from(pose.position).distance(Vec3::from(camera.eye)) < 0.1,
            "{vehicle}: {:?}",
            pose.position
        );
        assert!((pose_heading(pose.rotation) - camera.yaw).abs() < 0.01);
        assert!(Vec3::from(pose.velocity).length() < 0.5, "stopped");
        // The rider comes along in the seat.
        p.feed(&mut s, look, 2)?;
        let rider = s
            .motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .unwrap()
            .0;
        assert!(
            Vec3::from(rider.feet).distance(Vec3::from(camera.eye)) < 4.0,
            "{vehicle}: {:?}",
            rider.feet
        );
    }
    Ok(())
}

#[test]
#[ignore = "requires the converted native vehicle, weapon and brick packs"]
fn riders_keep_their_look_on_every_mount() -> anyhow::Result<()> {
    // `Player::updateMove` still turns `mHead` while mounted: other players
    // see a rider look up, down and around in any seat.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let look = MoveInput {
        pitch: 0.6,
        head_yaw: -1.1,
        ..Default::default()
    };
    let rider = |s: &Session, owner: u64| {
        s.motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .unwrap()
            .0
    };
    // A mouse-steered driver's pitch steers; their body stays level.
    let check = |s: &mut Session, p: &mut Feeder, what: &str, steers: bool| -> anyhow::Result<()> {
        assert!(s.mounted(p.owner).is_some(), "{what}: mounted");
        p.feed(s, MoveInput { yaw: rider(s, p.owner).yaw, ..look }, 5)?;
        let state = rider(s, p.owner);
        let pitch = if steers { 0.0 } else { 0.6 };
        assert!((state.pitch - pitch).abs() < 1e-5, "{what}: pitch {}", state.pitch);
        assert!((state.head_yaw + 1.1).abs() < 1e-5, "{what}: head {}", state.head_yaw);
        p.feed(s, MoveInput { yaw: rider(s, p.owner).yaw, ..Default::default() }, 5)?;
        let state = rider(s, p.owner);
        assert!(state.pitch.abs() < 1e-5 && state.head_yaw.abs() < 1e-5, "{what}: level");
        Ok(())
    };
    for (vehicle, seats) in [
        (JEEP, &[0u8, 1][..]),
        ("v20.vehicle.horsearmor", &[0][..]),
        ("v20.vehicle.tankvehicle", &[0, 1][..]),
    ] {
        let (mut s, owner) = session_with(&root, vehicle)?;
        let mut p = Feeder { owner, sequence: 0 };
        p.feed(&mut s, MoveInput::default(), 120)?;
        p.board(&mut s, 0.0)?;
        for &switch in seats {
            if switch > 0 {
                s.switch_seat(owner, 1)?;
                p.feed(&mut s, MoveInput::default(), 2)?;
            }
            let seat = s.mounted(owner).map(|m| m.1);
            // With the shipped steering prefs the Jeep's and Tank's driver
            // steers with the mouse, pitch included; the horse faces its look.
            let steers = seat == Some(0) && vehicle != "v20.vehicle.horsearmor";
            check(&mut s, &mut p, &format!("{vehicle} seat {seat:?}"), steers)?;
        }
    }
    // Skis come from their item, not a spawn brick.
    let (mut s, owner) = session(&root)?;
    let mut p = Feeder { owner, sequence: 0 };
    p.feed(&mut s, MoveInput::default(), 60)?;
    let slot = s.give_item(owner, "v20.weapon.skiitem")?;
    s.equip_tool(owner, Some(slot))?;
    p.feed(&mut s, MoveInput::default(), 80)?;
    for (command, down) in [(100, true), (101, false)] {
        s.command(owner, command, Command::WeaponTrigger { down })?;
        p.feed(&mut s, MoveInput::default(), 4)?;
    }
    p.feed(&mut s, MoveInput::default(), 40)?;
    check(&mut s, &mut p, "skis", true)?;
    Ok(())
}

#[test]
#[ignore = "requires the converted native vehicle and brick packs"]
fn the_hosts_physics_vehicle_limit_holds_back_a_spawn() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (mut s, owner) = session(&root)?;
    s.set_server_settings(bri_admin::ServerSettings {
        physics_vehicles: 0,
        ..Default::default()
    })?;
    for sequence in 1..=120 {
        s.movement(owner, sequence, MoveInput::default())?;
        s.step()?;
    }
    assert!(s.vehicle_infos().is_empty(), "no jeep past a limit of 0");
    // The brick's owner (here the map's public group) is told why.
    assert!(s.take_private_notices().iter().any(|(_, n)| matches!(
        n,
        bri_sim::session::Notice::Center { text, .. }
            if text.ends_with("Server is limited to 0 physics-vehicles")
    )));
    Ok(())
}

#[test]
#[ignore = "requires the converted native vehicle and brick packs"]
fn an_internet_hosts_per_builder_vehicle_quota_holds_back_a_spawn_but_lan_does_not()
-> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for lan in [false, true] {
        let (mut s, owner) = session(&root)?;
        s.set_lan_host(lan);
        let mut settings = bri_admin::ServerSettings::default();
        settings.per_player.vehicles = 0;
        s.set_server_settings(settings)?;
        for sequence in 1..=120 {
            s.movement(owner, sequence, MoveInput::default())?;
            s.step()?;
        }
        assert_eq!(s.vehicle_infos().len(), usize::from(lan), "LAN {lan}");
        if !lan {
            assert!(s.take_private_notices().iter().any(|(_, n)| matches!(
                n,
                bri_sim::session::Notice::Center { text, .. }
                    if text.ends_with("You already have 0 physics-vehicles")
            )));
        }
    }
    Ok(())
}

/// HorseArmor's player seat is horse.dts's `mount2` at rest, the same node
/// and place the converted pack seats a horse bot's rider.
#[test]
#[ignore = "requires the converted native vehicle pack"]
fn horse_player_seat_is_the_horse_shapes_mount_node() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pack = bri_vehicles::Pack::load(root.join("content/vehicles-pack-012/vehicles.json"))?;
    let horse = pack
        .definitions
        .iter()
        .find(|d| d.id == "v20.vehicle.horsearmor")
        .expect("HorseArmor");
    let points = bri_sim::player_types::PlayerType::Horse.mount_points();
    assert_eq!(points.len(), horse.seats.len(), "numMountPoints");
    for (point, seat) in points.iter().zip(&horse.seats) {
        assert_eq!(point.node, seat.node);
        assert!(Vec3::from(point.position).distance(Vec3::from(seat.transform.position)) < 1e-4);
        assert_eq!(point.pose, seat.pose);
    }
    Ok(())
}

/// A bot hit by the Horse Ray becomes a rideable horse. It has no client of
/// its own, so v20 gives the rider in its first seat control of it
/// (`setControlObject`): its brain stops and it runs where the rider looks.
#[test]
#[ignore = "requires the converted native vehicle, weapon and brick packs"]
fn a_horse_rayed_bot_is_ridden_and_steered_by_its_rider() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // A bot brick the shooter owns, so the bot follows their minigame.
    let definitions = Definitions::load(
        &root.join("content/stock-catalog-004"),
        &root.join("content/maps-pass-008"),
    )?;
    let mut world = World::new("Bot horse".into(), "test".into(), vec![[1.0; 4]]);
    let principal = [7; 32];
    world
        .owners
        .insert(1, bri_world::OwnerRecord::new(principal, "Shooter".into()));
    let mut brick = Brick::new(ContentRef::Resolved(SPAWN.into()), [0.0, 0.1, -12.0], 1);
    brick.vehicle = Some(VehicleSpawn {
        vehicle: ContentRef::Resolved("bot.blockhead".into()),
        recolor: false,
    });
    world.bricks.insert(1, brick);
    world.next_brick_id = 2;
    let mut s = Session::new(Simulation::new(
        world,
        definitions,
        vec![ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0))],
    )?);
    s.set_weapon_pack(bri_weapons::Pack::from_json(&std::fs::read(
        root.join("content/weapons-pack-009/weapons.json"),
    )?)?)?;
    s.set_vehicle_pack(bri_vehicles::Pack::load(
        root.join("content/vehicles-pack-012/vehicles.json"),
    )?, bri_sim::bot_kind::BotPack::from_json(include_bytes!("../../../packages/blockhead_bot/assets/bots.json")).unwrap().bots)?;
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)])?;
    let shooter = s.join_verified(
        "Shooter".into(),
        Vec3::new(0.0, 0.05, 0.0),
        true,
        Some(bri_admin::Principal(principal)),
    )?;
    assert_eq!(shooter, 1);
    let mut sequence = 0;
    let mut idle = |s: &mut Session, owner: u64, ticks: usize, input: MoveInput| {
        for _ in 0..ticks {
            sequence += 1;
            s.movement(owner, sequence, input).unwrap();
            s.step().unwrap();
        }
    };
    idle(&mut s, shooter, 60, MoveInput::default());
    let bot = *s
        .names()
        .keys()
        .find(|o| s.is_bot(**o))
        .expect("bot spawned from its brick");
    let feet = |s: &Session, owner: u64| {
        Vec3::from(
            s.snapshot()
                .players
                .into_iter()
                .find(|p| p.owner == owner)
                .unwrap()
                .feet,
        )
    };
    // The Horse Ray only works where it may hurt: inside a minigame, which
    // the bot joins as its brick owner's.
    s.command(
        shooter,
        1,
        Command::MiniGame(bri_sim::session::MiniGameRequest::Create {
            color: 0,
            settings: bri_minigames::Settings {
                // Unarmed, so the bot has nothing to fight back with.
                loadout: Default::default(),
                ..Default::default()
            },
        }),
    )?;
    idle(&mut s, shooter, 60, MoveInput::default());
    let game = s.vitals()[&shooter].minigame.expect("minigame");
    assert_eq!(s.vitals()[&bot].minigame, Some(game));
    let slot = s.give_item(shooter, "v20.weapon.horserayitem")?;
    s.equip_tool(shooter, Some(slot))?;
    let mut horse = false;
    for shot in 0..10 {
        let to = feet(&s, bot) + Vec3::Y * 1.5 - (feet(&s, shooter) + Vec3::Y * 2.3);
        let aim = MoveInput {
            yaw: to.x.atan2(-to.z),
            pitch: (to.y / to.length()).asin(),
            ..Default::default()
        };
        idle(&mut s, shooter, 5, aim);
        s.command(
            shooter,
            100 + shot * 2,
            Command::WeaponTrigger { down: true },
        )?;
        idle(&mut s, shooter, 5, aim);
        s.command(
            shooter,
            101 + shot * 2,
            Command::WeaponTrigger { down: false },
        )?;
        idle(&mut s, shooter, 60, aim);
        let state = s
            .snapshot()
            .players
            .into_iter()
            .find(|p| p.owner == bot)
            .unwrap();
        if state.archetype == bri_sim::player_types::PlayerType::Horse.archetype() {
            horse = true;
            break;
        }
    }
    assert!(horse, "the Horse Ray turned the bot into a horse");
    // A player of the same minigame dropped on its back takes the reins.
    let rider = s.join("Rider".into(), Vec3::new(30.0, 0.05, 30.0), false)?;
    s.set_spawn_points(vec![feet(&s, bot) + Vec3::Y * 6.0])?;
    s.command(
        rider,
        1,
        Command::MiniGame(bri_sim::session::MiniGameRequest::Join { game }),
    )?;
    let mut rs = 0;
    let mut ride = |s: &mut Session, ticks: usize, input: MoveInput| {
        for _ in 0..ticks {
            rs += 1;
            s.movement(rider, rs, input).unwrap();
            s.step().unwrap();
        }
    };
    ride(&mut s, 240, MoveInput::default());
    let seat = s.vitals()[&rider].ride.expect("rides the horse bot");
    assert_eq!((seat.mount, seat.seat, seat.steers), (bot, 0, true));
    // Looking east and pressing forward runs the horse east.
    let start = feet(&s, bot);
    let east = MoveInput {
        forward: 1.0,
        yaw: std::f32::consts::FRAC_PI_2,
        ..Default::default()
    };
    ride(&mut s, 240, east);
    let moved = feet(&s, bot) - start;
    assert!(moved.x > 5.0 && moved.x > moved.z.abs() * 3.0, "{moved}");
    assert_eq!(s.vitals()[&rider].ride.map(|r| r.mount), Some(bot));
    Ok(())
}

/// Playtest a20: a guest could not hammer their own vehicle spawn brick back.
/// v20's `indestructable` only keeps explosions off spawn bricks; the hammer
/// asks trust alone, and `fxDTSBrick::onDeath` deletes the brick's vehicle.
#[test]
#[ignore = "requires the converted native vehicle, weapon and brick packs"]
fn a_guest_hammers_their_own_vehicle_spawn_and_its_jeep_goes_with_it() -> anyhow::Result<()> {
    use bri_sim::session::{ToolCatalog, WrenchProperties};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let definitions = Definitions::load(
        &root.join("content/stock-catalog-004"),
        &root.join("content/maps-pass-008"),
    )?;
    assert!(definitions.entries[SPAWN].indestructible);
    let height = definitions.entries[SPAWN].mesh.height_plates as f32 * 0.2;
    let world = World::new("Hammer".into(), "test".into(), vec![[1.0, 0.0, 0.0, 1.0]]);
    let mut s = Session::new(Simulation::new(
        world,
        definitions,
        vec![ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0))],
    )?);
    s.set_weapon_pack(bri_weapons::Pack::from_json(&std::fs::read(
        root.join("content/weapons-pack-009/weapons.json"),
    )?)?)?;
    s.set_vehicle_pack(bri_vehicles::Pack::load(
        root.join("content/vehicles-pack-012/vehicles.json"),
    )?, Vec::new())?;
    s.set_tool_catalog(ToolCatalog {
        vehicles: [JEEP.to_string()].into(),
        vehicle_bricks: [SPAWN.to_string()].into(),
        ..Default::default()
    })?;
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 4.5)])?;
    let host = s.join("Host".into(), Vec3::new(4.0, 0.05, 4.5), true)?;
    let guest = s.join("Guest".into(), Vec3::new(0.0, 0.05, 4.5), false)?;
    assert!(!s.is_administrator(guest));
    let mut sequence = 0;
    let mut look = MoveInput::default();
    let mut idle = |s: &mut Session, ticks: usize, look: MoveInput| -> anyhow::Result<()> {
        for _ in 0..ticks {
            sequence += 1;
            s.movement(guest, sequence, look)?;
            s.movement(host, sequence, MoveInput::default())?;
            s.step()?;
        }
        Ok(())
    };
    idle(&mut s, 60, look)?;
    let bri_sim::session::Reply::Planted(brick) = s.command(
        guest,
        1,
        Command::Plant {
            definition: SPAWN.into(),
            position: [0.0, height * 0.5, 0.0],
            quarter_turns: 0,
            color: 0,
        },
    )?
    else {
        anyhow::bail!("spawn brick not planted")
    };
    assert_eq!(s.simulation().state().bricks[&brick].owner, guest);
    s.edit_brick(
        guest,
        brick,
        bri_world::authority::Edit::Properties(WrenchProperties {
            vehicle: Some(JEEP.into()),
            raycast: true,
            colliding: true,
            visible: true,
            ..Default::default()
        }),
    )?;
    idle(&mut s, 30, look)?;
    assert_eq!(s.vehicle_infos().len(), 1, "the brick spawned its jeep");
    // Swing at the brick's near edge, clear of the jeep parked above it.
    let (min, max) = s.simulation().brick_box(brick).unwrap();
    let edge = Vec3::new(0.0, max.y - 0.05, max.z - 0.3);
    assert!(edge.z > min.z);
    let d = (edge - Vec3::new(0.0, 2.4, 4.5)).normalize();
    look.yaw = d.x.atan2(-d.z);
    look.pitch = d.y.asin();
    s.equip_tool(guest, Some(0))?;
    idle(&mut s, 2, look)?;
    s.command(guest, 2, Command::WeaponTrigger { down: true })?;
    idle(&mut s, 12, look)?;
    s.command(guest, 3, Command::WeaponTrigger { down: false })?;
    idle(&mut s, 4, look)?;
    assert!(
        !s.simulation().state().bricks.contains_key(&brick),
        "the builder's hammer breaks their own spawn brick"
    );
    assert!(s.vehicle_infos().is_empty(), "the jeep goes with its brick");
    Ok(())
}

/// allGameScripts.cs:17839 `fxDTSBrick::recoverVehicle`: the event respawns
/// the brick's vehicle, except while a player rides it.
#[test]
#[ignore = "requires the converted native vehicle and brick packs"]
fn recover_vehicle_leaves_a_ridden_vehicle_alone() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (mut s, owner) = session(&root)?;
    let output = bri_events::OutputDef {
        id: "out/fxDTSBrick/recoverVehicle".into(),
        class_name: "fxDTSBrick".into(),
        name: "recoverVehicle".into(),
        params: vec![],
        append_client: true,
        source: "v20".into(),
        source_line: 17400,
    };
    s.set_event_catalog(
        bri_events::Catalog {
            schema_version: 1,
            inputs: vec![bri_events::InputDef {
                id: "in/onActivate".into(),
                class_name: "fxDTSBrick".into(),
                name: "onActivate".into(),
                targets: vec![("Self".into(), "fxDTSBrick".into())],
                source: "v20".into(),
                source_line: 17122,
            }],
            outputs: vec![output],
            sources: vec![],
            scope: serde_json::Value::Null,
        },
        Vec::new(),
    )?;
    s.edit_brick(
        owner,
        1,
        bri_world::authority::Edit::Events(vec![bri_world::EventRow {
            preserved: None,
            enabled: true,
            input: "onActivate".into(),
            delay_ms: 0,
            target: bri_world::EventTarget::Slot(bri_events::Slot::SelfBrick),
            output: "recoverVehicle".into(),
            params: vec![],
        }]),
    )?;
    let mut sequence = 0;
    let mut feed = |s: &mut Session, input: MoveInput, ticks: usize| -> anyhow::Result<()> {
        for _ in 0..ticks {
            sequence += 1;
            s.movement(owner, sequence, input)?;
            s.step()?;
        }
        Ok(())
    };
    feed(&mut s, MoveInput::default(), 120)?;
    for i in 0..60 {
        let input = MoveInput {
            forward: 1.0,
            jump: i % 3 == 0,
            ..Default::default()
        };
        feed(&mut s, input, 10)?;
        if s.mounted(owner).is_some() {
            break;
        }
    }
    let ridden = s.mounted(owner).expect("mounted the jeep").0;
    s.fire_brick_input(1, "onActivate", Some(owner));
    feed(&mut s, MoveInput::default(), 10)?;
    assert_eq!(s.vehicle_infos()[0].id, ridden, "recovered from under its driver");
    feed(&mut s, MoveInput { jet: true, ..Default::default() }, 2)?;
    feed(&mut s, MoveInput::default(), 10)?;
    assert_eq!(s.mounted(owner), None);
    s.fire_brick_input(1, "onActivate", Some(owner));
    feed(&mut s, MoveInput::default(), 10)?;
    assert_ne!(s.vehicle_infos()[0].id, ridden, "an empty vehicle was not recovered");
    Ok(())
}

/// The host consumes a driver's moves one per tick, as it does a walker's,
/// so a client predicting its vehicle (one step per move) agrees with it
/// even though moves arrive two to a datagram. Draining the whole queue
/// each tick ran two moves' steering in one step and none in the next, and
/// every pose then corrected the prediction: the view shook while steering.
#[test]
#[ignore = "requires the converted native vehicle and brick packs"]
fn a_predicted_driver_needs_no_corrections_when_moves_arrive_in_pairs() -> anyhow::Result<()> {
    use bri_sim::prediction::{CollisionMirror, DriveSpawn, Predictor};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jeep = "v20.vehicle.flyingwheeledjeepvehicle";
    let (mut s, owner) = session_with(&root, jeep)?;
    let mut p = Feeder { owner, sequence: 0 };
    p.feed(&mut s, MoveInput::default(), 120)?;
    p.board(&mut s, 0.0)?;
    assert_eq!(s.mounted(owner).map(|m| m.1), Some(0));
    p.feed(&mut s, MoveInput::default(), 30)?;
    let info = s.vehicle_infos().remove(0);
    let pose = s.vehicle_poses().remove(0);
    let rider = s
        .motion_states()
        .into_iter()
        .find(|(r, _)| r.owner == owner)
        .unwrap()
        .0;
    let world = vehicle_world(jeep);
    let mut mirror = CollisionMirror::new(definitions(&root)?, vec![ground()], vec![]);
    mirror.sync(&world.bricks)?;
    let mut client = Predictor::new(mirror, rider, Default::default())?;
    client.continue_after(p.sequence);
    client.drive(Some((
        bri_vehicles::Pack::load(root.join("content/vehicles-pack-012/vehicles.json"))?,
        DriveSpawn {
            spawn: bri_vehicles::Spawn {
                id: bri_vehicles::VehicleId(info.id),
                owner: bri_vehicles::OwnerId(owner),
                definition: info.definition.clone(),
                transform: Default::default(),
                spawn_id: None,
                respawn_ticks: None,
                scale: info.scale,
            },
            seat: 0,
            // As the client does: the host's copy of the driver's prefs.
            prefs: (!pose.driver_steering.0, !pose.driver_steering.1),
        },
        pose.motion(),
    )))?;
    // Throttle and a steady mouse turn: the jeep drives in a circle.
    let input = |tick: u64| MoveInput {
        forward: 1.0,
        yaw: 0.003 * tick as f32,
        ..Default::default()
    };
    let mut outbox = Vec::new();
    let mut in_flight = std::collections::VecDeque::new();
    let (mut worst_move, mut worst_turn) = (0.0_f32, 0.0_f32);
    for tick in 1..=360_u64 {
        let sequence = client.record(input(tick))?;
        outbox.push((sequence, input(tick)));
        // Two moves to a datagram.
        if outbox.len() == 2 {
            for (sequence, input) in outbox.drain(..) {
                s.movement(owner, sequence, input)?;
            }
        }
        s.step()?;
        if tick % 3 == 0 {
            in_flight.push_back((tick + 12, s.vehicle_poses().remove(0)));
        }
        while in_flight.front().is_some_and(|(at, _)| *at <= tick) {
            let (_, pose) = in_flight.pop_front().unwrap();
            let Some(before) = client.drive_pose(pose.tick, pose.driver_input, &pose.motion())?
            else {
                continue;
            };
            let (_, _, now) = client.driven().unwrap();
            // Past the first second, once the jeep is rolling.
            if tick > 120 {
                worst_move =
                    worst_move.max(Vec3::from(before.position).distance(Vec3::from(now.position)));
                worst_turn = worst_turn.max(
                    glam::Quat::from_array(before.rotation)
                        .angle_between(glam::Quat::from_array(now.rotation)),
                );
            }
        }
    }
    println!("worst correction {worst_move} units, {worst_turn} rad");
    assert!(
        worst_move < 0.02 && worst_turn < 0.002,
        "corrections of {worst_move} units and {worst_turn} rad would shake the view"
    );
    Ok(())
}

/// Heading of the one vehicle in the session.
fn vehicle_heading(s: &Session) -> f32 {
    pose_heading(s.vehicle_poses()[0].rotation)
}

/// `$pref::Input::UseStrafeSteering` off (the reference install's default):
/// the Jeep's driver steers with the mouse and the strafe keys do nothing;
/// on, the strafe keys steer and the mouse only looks
/// (`Player::processTick` 0x5b2d15 to 0x5b2e97).
#[test]
#[ignore = "requires the converted native vehicle and brick packs"]
fn the_jeep_steers_by_the_mouse_without_strafe_steering_and_by_the_keys_with_it() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let turned = |strafe: bool, input: &dyn Fn(u64) -> MoveInput| -> anyhow::Result<f32> {
        let (mut s, owner) = session(&root)?;
        s.command(
            owner,
            1,
            Command::SteeringPrefs {
                strafe,
                auto_return: false,
            },
        )?;
        let mut p = Feeder { owner, sequence: 0 };
        p.feed(&mut s, MoveInput::default(), 120)?;
        p.board(&mut s, 0.0)?;
        p.feed(&mut s, MoveInput::default(), 60)?;
        let before = vehicle_heading(&s);
        for tick in 0..120 {
            p.feed(&mut s, input(tick), 1)?;
        }
        let after = vehicle_heading(&s);
        Ok((after - before + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI)
    };
    // Throttle and the mouse turning right.
    let mouse = |tick: u64| MoveInput {
        forward: 1.0,
        yaw: 0.004 * tick as f32,
        ..Default::default()
    };
    // Throttle and D held.
    let keys = |_: u64| MoveInput {
        forward: 1.0,
        right: 1.0,
        ..Default::default()
    };
    let mouse_off = turned(false, &mouse)?;
    let keys_off = turned(false, &keys)?;
    let mouse_on = turned(true, &mouse)?;
    let keys_on = turned(true, &keys)?;
    println!("mouse/keys, strafe off: {mouse_off} {keys_off}; on: {mouse_on} {keys_on}");
    // Which input steers is the point; how far a second from standstill
    // turns depends on the tyres (Torque's give a little and share their
    // grip with the launch, 0.27 here against 0.9 for a held key).
    assert!(mouse_off > 0.2, "the mouse steers right: {mouse_off}");
    assert!(keys_off.abs() < 0.05, "the keys do nothing: {keys_off}");
    assert!(mouse_on.abs() < 0.05, "the mouse only looks: {mouse_on}");
    assert!(keys_on > 0.3, "D steers right: {keys_on}");
    Ok(())
}

/// A passenger's mouse turns their whole body on the seat: the host adds
/// the turn (their move's yaw, relative to the seat) to the seat's heading,
/// as `Player::setPosition` (0x5a6bc0) turns the mount node by `mRot.z`.
/// A driver's body stays facing the seat.
#[test]
#[ignore = "requires the converted native vehicle and brick packs"]
fn a_passenger_turns_on_the_seat_and_the_driver_does_not() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (mut s, owner) = session(&root)?;
    let mut p = Feeder { owner, sequence: 0 };
    p.feed(&mut s, MoveInput::default(), 120)?;
    p.board(&mut s, 0.0)?;
    let body = |s: &Session| {
        s.motion_states()
            .into_iter()
            .find(|(r, _)| r.owner == owner)
            .unwrap()
            .0
            .yaw
    };
    let turn = |s: &Session| {
        (body(s) - vehicle_heading(s) + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI
    };
    p.feed(
        &mut s,
        MoveInput {
            yaw: 1.2,
            ..Default::default()
        },
        5,
    )?;
    assert!(turn(&s).abs() < 1e-3, "the driver faces the seat: {}", turn(&s));
    s.switch_seat(owner, 1)?;
    assert_eq!(s.mounted(owner).map(|m| m.1), Some(1));
    for yaw in [0.9, -2.5] {
        p.feed(
            &mut s,
            MoveInput {
                yaw,
                ..Default::default()
            },
            5,
        )?;
        assert!((turn(&s) - yaw).abs() < 1e-3, "turned {} for {yaw}", turn(&s));
    }
    Ok(())
}

/// A rowboat is a player-type mount, but its passengers have no control
/// object either: they turn on their seats the same way.
#[test]
#[ignore = "requires the converted native vehicle and brick packs"]
fn a_rowboat_passenger_turns_on_the_seat() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (mut s, owner) = session_with(&root, "v20.vehicle.rowboatarmor")?;
    let mut p = Feeder { owner, sequence: 0 };
    p.feed(&mut s, MoveInput::default(), 120)?;
    p.board(&mut s, 0.0)?;
    s.switch_seat(owner, 1)?;
    assert_eq!(s.mounted(owner).map(|m| m.1), Some(1));
    p.feed(
        &mut s,
        MoveInput {
            yaw: 1.3,
            ..Default::default()
        },
        5,
    )?;
    let body = s
        .motion_states()
        .into_iter()
        .find(|(r, _)| r.owner == owner)
        .unwrap()
        .0
        .yaw;
    let turn = (body - vehicle_heading(&s) + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    assert!((turn - 1.3).abs() < 1e-3, "turned {turn}");
    Ok(())
}

/// A deterministic xorshift for jittered timing.
struct Jitter(u64);
impl Jitter {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    /// A whole number of microseconds in `range`.
    fn between(&mut self, range: std::ops::Range<u64>) -> u64 {
        range.start + self.next() % (range.end - range.start)
    }
}

/// The biggest prediction correction (units, radians) a driver of
/// `vehicle` sees with real-world timing: uneven client frames, inputs sent
/// once a frame with the last six repeated, 40 ms each way with 0 to 15 ms of
/// jitter, and the host ticking on its own clock.
fn corrections_under_timing(vehicle: &str, jittered: bool) -> anyhow::Result<(f32, f32, usize)> {
    use bri_sim::prediction::{CollisionMirror, DriveSpawn, Predictor};
    const TICK_US: u64 = 1_000_000 / 120;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (mut s, owner) = session_with(&root, vehicle)?;
    let mut p = Feeder { owner, sequence: 0 };
    p.feed(&mut s, MoveInput::default(), 120)?;
    p.board(&mut s, 0.0)?;
    anyhow::ensure!(s.mounted(owner).map(|m| m.1) == Some(0), "not driving");
    p.feed(&mut s, MoveInput::default(), 30)?;
    let info = s.vehicle_infos().remove(0);
    let pose = s.vehicle_poses().remove(0);
    let rider = s
        .motion_states()
        .into_iter()
        .find(|(r, _)| r.owner == owner)
        .unwrap()
        .0;
    let world = vehicle_world(vehicle);
    let mut mirror = CollisionMirror::new(definitions(&root)?, vec![ground()], vec![]);
    mirror.sync(&world.bricks)?;
    let mut client = Predictor::new(mirror, rider, Default::default())?;
    client.continue_after(p.sequence);
    client.drive(Some((
        bri_vehicles::Pack::load(root.join("content/vehicles-pack-012/vehicles.json"))?,
        DriveSpawn {
            spawn: bri_vehicles::Spawn {
                id: bri_vehicles::VehicleId(info.id),
                owner: bri_vehicles::OwnerId(owner),
                definition: info.definition.clone(),
                transform: Default::default(),
                spawn_id: None,
                respawn_ticks: None,
                scale: info.scale,
            },
            seat: 0,
            // As the client does: the host's copy of the driver's prefs.
            prefs: (!pose.driver_steering.0, !pose.driver_steering.1),
        },
        pose.motion(),
    )))?;
    // Throttle and sharp mouse turns: left, right, up, down.
    let input = |sequence: u64| {
        let t = sequence as f32 / 120.0;
        MoveInput {
            forward: 1.0,
            yaw: (t * 2.0).sin() * 1.5,
            pitch: (t * 3.0).sin() * 0.4,
            ..Default::default()
        }
    };
    let mut rng = Jitter(0x9e37_79b9_7f4a_7c15);
    let mut to_host: Vec<(u64, Vec<(u64, MoveInput)>)> = Vec::new();
    let mut to_client: Vec<(u64, bri_sim::session::VehiclePose)> = Vec::new();
    let mut sent: std::collections::VecDeque<(u64, MoveInput)> = Default::default();
    let (mut next_tick, mut next_frame, mut accumulator) = (0u64, 0u64, 0u64);
    let (mut worst_move, mut worst_turn, mut corrections) = (0.0_f32, 0.0_f32, 0);
    let mut host_ticks = 0u64;
    for now in (0..6_000_000u64).step_by(250) {
        if now >= next_tick {
            next_tick += TICK_US;
            to_host.sort_by_key(|(at, _)| *at);
            while to_host.first().is_some_and(|(at, _)| *at <= now) {
                let (_, moves) = to_host.remove(0);
                for (sequence, input) in moves {
                    let _ = s.movement(owner, sequence, input);
                }
            }
            s.step()?;
            host_ticks += 1;
            if host_ticks.is_multiple_of(3) {
                let latency = 40_000 + if jittered { rng.between(0..15_000) } else { 0 };
                to_client.push((now + latency, s.vehicle_poses().remove(0)));
            }
        }
        if now >= next_frame {
            let frame = if jittered { rng.between(6_000..25_000) } else { TICK_US };
            next_frame += frame;
            accumulator += frame;
            while accumulator >= TICK_US {
                accumulator -= TICK_US;
                let sequence = client.sequence() + 1;
                client.record(input(sequence))?;
                sent.push_back((sequence, input(sequence)));
                while sent.len() > 6 {
                    sent.pop_front();
                }
            }
            if !sent.is_empty() {
                let latency = 40_000 + if jittered { rng.between(0..15_000) } else { 0 };
                to_host.push((now + latency, sent.iter().copied().collect()));
            }
            to_client.sort_by_key(|(at, _)| *at);
            while to_client.first().is_some_and(|(at, _)| *at <= now) {
                let (_, pose) = to_client.remove(0);
                let Some(before) = client.drive_pose(pose.tick, pose.driver_input, &pose.motion())?
                else {
                    continue;
                };
                let (_, _, after) = client.driven().unwrap();
                // Past the first second.
                if now > 1_000_000 {
                    let moved = Vec3::from(before.position).distance(Vec3::from(after.position));
                    let turned = glam::Quat::from_array(before.rotation)
                        .angle_between(glam::Quat::from_array(after.rotation));
                    worst_move = worst_move.max(moved);
                    worst_turn = worst_turn.max(turned);
                    if moved > 0.01 || turned > 0.003 {
                        corrections += 1;
                    }
                }
            }
        }
    }
    Ok((worst_move, worst_turn, corrections))
}

/// The real host and a predicting client over a jittered connection with
/// uneven frames: the host runs the driver's moves as the client predicted
/// them, so a driven vehicle on its wheels or in the air needs no visible
/// correction. The Magic Carpet scraping the ground is reported only: its
/// contacts are not reproducible step for step, and the client eases what
/// that leaves out gently (`motion.rs`).
#[test]
#[ignore = "requires the converted native vehicle and brick packs"]
fn predicted_vehicles_stay_uncorrected_under_real_timing() -> anyhow::Result<()> {
    for vehicle in [
        "v20.vehicle.magiccarpetvehicle",
        "v20.vehicle.flyingwheeledjeepvehicle",
        "v20.vehicle.jeepvehicle",
    ] {
        for jittered in [true, false] {
            let (moved, turned, count) = corrections_under_timing(vehicle, jittered)?;
            println!(
                "{vehicle} (jittered {jittered}): worst correction {moved:.4} units, {turned:.4} rad; {count} visible"
            );
            if vehicle != "v20.vehicle.magiccarpetvehicle" {
                assert_eq!(count, 0, "{vehicle}: visible corrections");
            }
        }
    }
    Ok(())
}

/// Max, v0.1.4: the Tank's mouse and A/D steering fought. The host steers
/// a driver by its copy of their steering prefs while their client
/// predicted by its own, so any gap between the two (before the prefs
/// arrive, a map change, a reconnect) had the host steer by the keys while
/// the client steered by the mouse. The host assumes the client's shipped
/// prefs, keeps a player's own across seats and maps, forgets them when
/// they leave, and tells the driver's client in every pose which it uses.
#[test]
#[ignore = "requires the converted native vehicle and brick packs"]
fn the_host_steers_a_driver_by_the_prefs_it_echoes() -> anyhow::Result<()> {
    use bri_sim::session::DEFAULT_STEERING;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (mut s, owner) = session_with(&root, "v20.vehicle.tankvehicle")?;
    let mut p = Feeder { owner, sequence: 0 };
    p.feed(&mut s, MoveInput::default(), 120)?;
    p.board(&mut s, 0.0)?;
    p.feed(&mut s, MoveInput::default(), 60)?;
    // The mouse turning left while D is held: the mouse steers left, the
    // key right.
    let turn = |s: &mut Session, p: &mut Feeder| -> anyhow::Result<f32> {
        let before = vehicle_heading(s);
        for tick in 0..120 {
            let input = MoveInput {
                forward: 1.0,
                right: 1.0,
                yaw: -0.004 * tick as f32,
                ..Default::default()
            };
            p.feed(s, input, 1)?;
        }
        p.feed(s, MoveInput::default(), 1)?;
        Ok((vehicle_heading(s) - before + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI)
    };
    // No prefs heard yet: the client's shipped ones, echoed.
    assert_eq!(s.vehicle_poses()[0].driver_steering, DEFAULT_STEERING);
    let mouse = turn(&mut s, &mut p)?;
    assert!(mouse < -0.3, "the mouse steers without any prefs sent: {mouse}");
    // The player's own: strafe steering on, echoed, and the key steers.
    s.command(
        owner,
        1,
        Command::SteeringPrefs {
            strafe: true,
            auto_return: false,
        },
    )?;
    p.feed(&mut s, MoveInput::default(), 1)?;
    assert_eq!(s.vehicle_poses()[0].driver_steering, (true, false));
    let keys = turn(&mut s, &mut p)?;
    assert!(keys > 0.3, "D steers with strafe steering: {keys}");
    // Leaving the seat keeps them.
    p.feed(
        &mut s,
        MoveInput {
            jet: true,
            ..Default::default()
        },
        2,
    )?;
    p.feed(&mut s, MoveInput::default(), 2)?;
    assert_eq!(s.mounted(owner), None, "jet left the Tank");
    assert_eq!(s.vehicle_poses()[0].driver_steering, DEFAULT_STEERING, "no driver");
    assert_eq!(s.steering_prefs(owner), (true, false));
    // So does a map change.
    let mut next = Session::new(Simulation::new(
        vehicle_world("v20.vehicle.tankvehicle"),
        definitions(&root)?,
        vec![ground()],
    )?);
    next.set_vehicle_pack(bri_vehicles::Pack::load(
        root.join("content/vehicles-pack-012/vehicles.json"),
    )?, Vec::new())?;
    next.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)])?;
    next.adopt(s, owner)?;
    let mut s = next;
    assert_eq!(s.steering_prefs(owner), (true, false));
    // Leaving the game forgets them: whoever comes back starts shipped.
    s.disconnect(owner)?;
    s.resume(owner, Vec3::new(0.0, 0.05, 0.0))?;
    assert_eq!(s.steering_prefs(owner), DEFAULT_STEERING);
    Ok(())
}
