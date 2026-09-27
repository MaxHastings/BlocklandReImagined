//! Checkpoints, teledoors, treasure chests and water bricks with the
//! converted stock brick definitions.
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    session::{Command, Notice, Reply, Session},
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;
use std::path::Path;

const CHECKPOINT: &str = "v20/brick/brickcheckpointdata";
const TELEDOOR: &str = "v20/brick/brickteledoordata";
const CHEST: &str = "v20/brick/bricktreasurechestdata";
const CHEST_OPEN: &str = "v20/brick/bricktreasurechestopendata";
const WATER: &str = "v20/brick/brick8xwaterdata";

struct Harness {
    s: Session,
    owner: u64,
    sequence: u64,
}
impl Harness {
    fn new() -> anyhow::Result<Self> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let definitions = Definitions::load(
            &root.join("content/stock-catalog-004"),
            &root.join("content/maps-pass-007"),
        )?;
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

#[test]
#[ignore = "requires the converted native brick catalog"]
fn checkpoint_sets_the_respawn_point() -> anyhow::Result<()> {
    let mut h = Harness::new()?;
    // Checkpoint two studs ahead (-Z is forward at yaw 0).
    let checkpoint = h.plant(CHECKPOINT, 0, -6, 0)?;
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

#[test]
#[ignore = "requires the converted native brick catalog"]
fn consecutive_teledoors_pair_and_carry_players_through() -> anyhow::Result<()> {
    let mut h = Harness::new()?;
    let a = h.plant(TELEDOOR, 0, -8, 0)?;
    let b = h.plant(TELEDOOR, 40, 0, 0)?;
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
    let c = h.plant(TELEDOOR, -20, 0, 0)?;
    assert_ne!(
        h.s.simulation().state().bricks[&c].name,
        h.s.simulation().state().bricks[&a].name
    );
    Ok(())
}

#[test]
#[ignore = "requires the converted native brick catalog"]
fn treasure_chest_opens_once_per_player_and_closes() -> anyhow::Result<()> {
    let mut h = Harness::new()?;
    let chest = h.plant(CHEST, 0, -4, 0)?;
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
    assert_eq!(definition(&h), CHEST_OPEN);
    assert!(
        h.bottom_prints()
            .iter()
            .any(|p| p.contains("found the treasure chest"))
    );
    h.run(MoveInput::default(), 250)?;
    assert_eq!(definition(&h), CHEST, "closes after two seconds");
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
    assert_eq!(definition(&h), CHEST, "a found chest stays closed");
    assert!(
        h.bottom_prints()
            .iter()
            .any(|p| p.contains("already opened"))
    );
    Ok(())
}

#[test]
#[ignore = "requires the converted native brick catalog"]
fn water_bricks_are_swimmable_not_solid() -> anyhow::Result<()> {
    let mut h = Harness::new()?;
    let water = h.plant(WATER, -4, -20, 0)?;
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
