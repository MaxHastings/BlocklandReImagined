//! Checkpoints, teledoors, treasure chests and water bricks, with made-up
//! bricks and with the converted stock brick definitions.
use bri_sim::{
    player::MoveInput,
    session::{Command, Notice, Reply, Session},
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;
mod common;
use common::{BrickRole, Fixture};

struct Harness {
    s: Session,
    owner: u64,
    sequence: u64,
}
impl Harness {
    fn new(f: &Fixture) -> anyhow::Result<Self> {
        Self::with(f.bricks())
    }
    fn with(definitions: bri_sim::definitions::Definitions) -> anyhow::Result<Self> {
        let world = World::new("Special".into(), "test".into(), vec![[1.0; 4]]);
        let mut s = Session::new(Simulation::new(
            world,
            definitions,
            vec![
                ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )?);
        s.set_spawn_points(vec![Vec3::new(0.25, 0.05, 0.25)])?;
        let owner = s.join("Builder".into(), Vec3::new(0.25, 0.05, 0.25), true)?;
        let mut h = Self {
            s,
            owner,
            sequence: 0,
        };
        h.run(MoveInput::default(), 60)?;
        Ok(h)
    }
    fn run(&mut self, input: MoveInput, ticks: usize) -> anyhow::Result<()> {
        for _ in 0..ticks {
            self.sequence += 1;
            self.s.movement(self.owner, self.sequence, input)?;
            self.s.step()?;
        }
        Ok(())
    }
    fn command(&mut self, command: Command) -> anyhow::Result<Reply> {
        self.sequence += 1;
        self.s.command(self.owner, self.sequence, command)
    }
    /// Plant on the floor with the brick's lower corner at stud cell (x, z).
    fn plant(&mut self, definition: &str, x: i32, z: i32, turns: u8) -> anyhow::Result<u64> {
        let mesh = &self.s.simulation().definitions.entries[definition].mesh;
        let (w, d) = if turns % 2 == 1 {
            (mesh.footprint_studs[1], mesh.footprint_studs[0])
        } else {
            (mesh.footprint_studs[0], mesh.footprint_studs[1])
        };
        let position = [
            x as f32 * 0.5 + w as f32 * 0.25,
            mesh.height_plates as f32 * 0.1,
            z as f32 * 0.5 + d as f32 * 0.25,
        ];
        let Reply::Planted(id) = self.command(Command::Plant {
            definition: definition.into(),
            position,
            quarter_turns: turns,
            color: 0,
        })?
        else {
            anyhow::bail!("not planted")
        };
        Ok(id)
    }
    fn feet(&self) -> Vec3 {
        let (state, _) = self
            .s
            .motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == self.owner)
            .unwrap();
        Vec3::from(state.feet)
    }
    fn walk(&mut self, yaw: f32, ticks: usize) -> anyhow::Result<()> {
        self.run(
            MoveInput {
                forward: 1.0,
                yaw,
                ..Default::default()
            },
            ticks,
        )
    }
    fn bottom_prints(&mut self) -> Vec<String> {
        self.s
            .take_private_notices()
            .into_iter()
            .filter_map(|(_, n)| match n {
                Notice::Bottom { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    }
}

on_both! {
fn checkpoint_sets_the_respawn_point(f: &Fixture) -> anyhow::Result<()> {
    let mut h = Harness::new(f)?;
    // Checkpoint two studs ahead (-Z is forward at yaw 0).
    let checkpoint = h.plant(f.brick(BrickRole::Checkpoint), 0, -6, 0)?;
    h.walk(0.0, 120)?;
    let prints = h.bottom_prints();
    assert!(
        prints.iter().any(|p| p.contains("Checkpoint reached")),
        "{prints:?} at {}",
        h.feet()
    );
    // Walk away, die, respawn: back on the checkpoint.
    h.walk(std::f32::consts::PI, 120)?;
    h.command(Command::Suicide)?;
    h.run(MoveInput::default(), 400)?;
    h.command(Command::Respawn).ok();
    h.run(MoveInput::default(), 30)?;
    let center = Vec3::from(h.s.simulation().state().bricks[&checkpoint].position);
    let feet = h.feet();
    assert!(
        Vec3::new(feet.x - center.x, 0.0, feet.z - center.z).length() < 1.0,
        "respawned at {feet}, checkpoint {center}"
    );
    // Clearing it sends the player back to the map spawn.
    h.command(Command::ClearCheckpoint)?;
    h.run(MoveInput::default(), 10)?;
    assert!(h.feet().distance(Vec3::new(0.25, 0.05, 0.25)) < 1.0);
    Ok(())
}
}

on_both! {
fn returning_players_appear_where_a_respawn_would_put_them(f: &Fixture) -> anyhow::Result<()> {
    let mut h = Harness::new(f)?;
    let spawn = h.plant(f.brick(BrickRole::SpawnPoint), 10, 10, 0)?;
    let center = Vec3::from(h.s.simulation().state().bricks[&spawn].position);
    // Rejoining offers the map drop point; the player's own spawn brick
    // wins, exactly as it does for a respawn.
    h.s.disconnect(h.owner)?;
    h.s.resume(h.owner, Vec3::new(-20.0, 0.05, -20.0))?;
    h.run(MoveInput::default(), 10)?;
    let feet = h.feet();
    assert!(
        Vec3::new(feet.x - center.x, 0.0, feet.z - center.z).length() < 1.0,
        "rejoined at {feet}, spawn brick {center}"
    );
    Ok(())
}
}

on_both! {
fn every_spawn_rotation_is_valid_before_the_first_post_spawn_weapon_tick(f: &Fixture) -> anyhow::Result<()> {
    for turns in 0..4 {
        let mut h = Harness::new(f)?;
        let spawn = h.plant(f.brick(BrickRole::SpawnPoint), 10, 10, turns)?;
        let center = Vec3::from(h.s.simulation().state().bricks[&spawn].position);
        h.s.disconnect(h.owner)?;
        h.s.resume(h.owner, Vec3::new(-20.0, 0.05, -20.0))?;
        let (state, _) = h.s.motion_states().into_iter().find(|(p, _)| p.owner == h.owner).unwrap();
        let feet = Vec3::from(state.feet);
        assert!(Vec3::new(feet.x - center.x, 0., feet.z - center.z).length() < 1.);
        assert!(state.yaw.abs() <= std::f32::consts::PI,
            "spawn rotation {turns}: heading {} is outside the weapon/input range", state.yaw);
        let expected = -f32::from(turns) * std::f32::consts::FRAC_PI_2;
        let facing = Vec3::new(expected.sin(), 0., -expected.cos());
        assert!(state.forward().distance(facing) < 1e-5,
            "normalization must preserve the spawn's facing");
        // No movement/look command may be needed to repair the spawn heading.
        h.s.step()?;
    }
    Ok(())
}
}

on_both! {
fn consecutive_teledoors_pair_and_carry_players_through(f: &Fixture) -> anyhow::Result<()> {
    let mut h = Harness::new(f)?;
    let a = h.plant(f.brick(BrickRole::Teledoor), 0, -8, 0)?;
    let b = h.plant(f.brick(BrickRole::Teledoor), 40, 0, 0)?;
    let bricks = &h.s.simulation().state().bricks;
    assert!(bricks[&a].name.is_some());
    assert_eq!(bricks[&a].name, bricks[&b].name, "planted doors pair up");
    let exit = Vec3::from(bricks[&b].position);
    // Walk into door A.
    let mut through = false;
    for _ in 0..240 {
        h.walk(0.0, 1)?;
        if Vec3::new(h.feet().x - exit.x, 0.0, h.feet().z - exit.z).length() < 3.0 {
            through = true;
            break;
        }
    }
    assert!(through, "ended at {} (exit door {exit})", h.feet());
    // A third door starts a new pair.
    let c = h.plant(f.brick(BrickRole::Teledoor), -20, 0, 0)?;
    assert_ne!(
        h.s.simulation().state().bricks[&c].name,
        h.s.simulation().state().bricks[&a].name
    );
    Ok(())
}
}

on_both! {
fn treasure_chest_opens_once_per_player_and_closes(f: &Fixture) -> anyhow::Result<()> {
    let mut h = Harness::new(f)?;
    let chest = h.plant(f.brick(BrickRole::TreasureChest), 0, -4, 0)?;
    let definition = |h: &Harness| match &h.s.simulation().state().bricks[&chest].definition {
        bri_world::ContentRef::Resolved(id) => id.clone(),
        _ => String::new(),
    };
    // Face the chest and click it.
    let target = Vec3::from(h.s.simulation().state().bricks[&chest].position);
    let eye = h.feet() + Vec3::Y * 2.1;
    let d = (target - eye).normalize();
    h.sequence += 1;
    h.s.command_with_aim(
        h.owner,
        h.sequence,
        Command::Activate,
        Some(bri_sim::session::ActionAim {
            yaw: d.x.atan2(-d.z),
            pitch: d.y.asin(),
        }),
    )?;
    assert_eq!(definition(&h), f.brick(BrickRole::TreasureChestOpen));
    assert!(
        h.bottom_prints()
            .iter()
            .any(|p| p.contains("found the treasure chest"))
    );
    h.run(MoveInput::default(), 250)?;
    assert_eq!(definition(&h), f.brick(BrickRole::TreasureChest), "closes after two seconds");
    h.sequence += 1;
    h.s.command_with_aim(
        h.owner,
        h.sequence,
        Command::Activate,
        Some(bri_sim::session::ActionAim {
            yaw: d.x.atan2(-d.z),
            pitch: d.y.asin(),
        }),
    )?;
    assert_eq!(definition(&h), f.brick(BrickRole::TreasureChest), "a found chest stays closed");
    assert!(
        h.bottom_prints()
            .iter()
            .any(|p| p.contains("already opened"))
    );
    Ok(())
}
}

on_both! {
fn water_bricks_are_swimmable_not_solid(f: &Fixture) -> anyhow::Result<()> {
    let mut h = Harness::new(f)?;
    let water = h.plant(f.brick(BrickRole::DeepWater), -4, -20, 0)?;
    let (min, max) = h.s.simulation().brick_box(water).unwrap();
    // `createWaterZone`'s box sits 0.15 below the brick and 0.05 under its top.
    assert!(h.s.simulation().liquids().iter().any(|w| {
        (w.max[1] - (max.y - 0.05)).abs() < 1e-4 && (w.min[1] - (min.y - 0.15)).abs() < 1e-4
    }));
    h.s.take_cues();
    // Walking in sinks into the water instead of standing on top of it.
    let mut inside = None;
    for _ in 0..600 {
        h.walk(0.0, 1)?;
        if h.feet().z < (min.z + max.z) * 0.5 {
            inside = Some(h.feet());
            break;
        }
    }
    let feet = inside.expect("reached the water");
    assert!(
        feet.y < max.y - 0.5,
        "player at {feet} should be inside the water {min}..{max}"
    );
    // The brick is taller than the player, so walking in covers the body at
    // once. v20 splashes only on partial coverage, so this entry is silent.
    let splashes: Vec<_> = h
        .s
        .take_cues()
        .into_iter()
        .filter_map(|c| match c.kind {
            bri_sim::presentation::CueKind::Water { entered, speed, .. } => Some((entered, speed)),
            _ => None,
        })
        .collect();
    assert!(splashes.is_empty(), "{splashes:?}");
    Ok(())
}
}

/// A brick whose catalog names a click swap (an Add-On's door) turns into
/// the brick for the side it is clicked from, at most every quarter second.
#[test]
fn a_click_swaps_a_brick_by_the_side_it_is_clicked_from() -> anyhow::Result<()> {
    use bri_content::brick::Swap;
    use bri_sim::testing as t;
    let mut definitions = t::definitions();
    let mut door = definitions.entries[t::BRICK].clone();
    door.mesh.footprint_studs[1] += 2;
    door.mesh.attachment_rows = vec![
        "b".repeat(door.mesh.footprint_studs[0] as usize);
        door.mesh.footprint_studs[1] as usize
    ];
    for id in ["test/brick/door-front", "test/brick/door-back"] {
        definitions.entries.insert(id.into(), door.clone());
    }
    let swap = |front: &str, back: &str| Swap {
        front: front.into(),
        back: back.into(),
    };
    let mut h = Harness::with(definitions)?;
    let mut tools = bri_sim::session::ToolCatalog::default();
    tools.swaps.insert(
        t::BRICK.into(),
        swap("test/brick/door-front", "test/brick/door-back"),
    );
    tools
        .swaps
        .insert("test/brick/door-back".into(), swap(t::BRICK, t::BRICK));
    for id in [t::BRICK, "test/brick/door-back"] {
        tools
            .swap_sounds
            .insert(id.into(), "v20/sound/brickchange".into());
    }
    h.s.set_tool_catalog(tools)?;
    let brick = h.plant(t::BRICK, 0, -4, 0)?;
    h.s.take_cues();
    let definition = |h: &Harness| match &h.s.simulation().state().bricks[&brick].definition {
        bri_world::ContentRef::Resolved(id) => id.clone(),
        _ => String::new(),
    };
    let click = |h: &mut Harness| -> anyhow::Result<()> {
        let target = Vec3::from(h.s.simulation().state().bricks[&brick].position);
        let d = (target - (h.feet() + Vec3::Y * 2.1)).normalize();
        h.sequence += 1;
        h.s.command_with_aim(
            h.owner,
            h.sequence,
            Command::Activate,
            Some(bri_sim::session::ActionAim {
                yaw: d.x.atan2(-d.z),
                pitch: d.y.asin(),
            }),
        )?;
        Ok(())
    };
    // The player stands behind it (+Z of an unturned brick).
    click(&mut h)?;
    assert_eq!(definition(&h), "test/brick/door-back");
    assert!(h.s.take_cues().iter().any(|c| matches!(&c.kind, bri_sim::presentation::CueKind::WeaponSound { profile } if profile == "v20/sound/brickchange")));
    click(&mut h)?;
    assert_eq!(
        definition(&h),
        "test/brick/door-back",
        "within the cooldown"
    );
    h.run(MoveInput::default(), 40)?;
    click(&mut h)?;
    assert_eq!(definition(&h), t::BRICK, "and back");
    assert!(h.s.take_cues().iter().any(|c| matches!(&c.kind, bri_sim::presentation::CueKind::WeaponSound { profile } if profile == "v20/sound/brickchange")));
    Ok(())
}
