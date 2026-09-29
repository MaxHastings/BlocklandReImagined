//! Native content for the v20 Tutorial map: the mission's trigger zones, read
//! from the converted scene, and the tutorial pack converted from
//! `Map_Tutorial` (its brick layouts, target practice schedule and target
//! models). The rules that use them live in `session::tutorial`.
use anyhow::{Context, Result, ensure};
use bri_content::scene::{Kind, Scene};
use bri_content::shape::Shape;
pub use bri_content::tutorial::{
    PACK_INDEX, PackIndex, TARGET_HIT_SHAPE, TARGET_M_HIT_SHAPE, TARGET_M_SHAPE, TARGET_SHAPE,
    TARGET_SKINS, TargetLaunch,
};
use bri_world::World;
use glam::{Mat4, Quat, Vec3};
use rapier3d::prelude::SharedShape;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MAP_ID: &str = "v20/add-ons/map_tutorial/tutorial.mis";
const MAX_INDEX_BYTES: u64 = 1 << 20;
const MAX_SHAPE_BYTES: u64 = 4 << 20;
const TICKS_PER_SECOND: f64 = 120.0;

/// `launchTarget` places a target at x = -44.8628 in one of three lanes
/// (`-71.371`, `-64.8711`, `-58.8758` on Torque's y) at height 94.4225, and
/// `scrollTarget` deletes it once x passes -32.
pub const TARGET_START_X: f32 = -44.8628;
pub const TARGET_END_X: f32 = -32.0;
const TARGET_Y: f32 = 94.4225;
const TARGET_LANES_Z: [f32; 3] = [71.371, 64.8711, 58.8758];
/// `scrollTarget` per speed 1-5: the distance of each step over its period.
/// Targets move continuously at that rate instead of in 20-30 ms steps.
const TARGET_SPEEDS: [f32; 5] = [
    0.06 / 0.030,
    0.08 / 0.025,
    0.09 / 0.025,
    0.1 / 0.020,
    0.17 / 0.020,
];

/// A launched target (`launchTarget`), as the server keeps it and clients
/// draw it. Its motion follows from its launch, so it only changes when it
/// is hit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetView {
    pub id: u32,
    /// Lane 1-3.
    pub row: u8,
    /// Scroll speed 1-5.
    pub speed: u8,
    /// `TutorialTargetM` rather than `TutorialTarget`.
    pub marked: bool,
    /// Index into [`TARGET_SKINS`].
    pub skin: u8,
    /// Server tick it was launched on.
    pub launched: u64,
    /// `isDead`: shot, and showing its `...Hit` datablock.
    pub hit: bool,
}
impl TargetView {
    pub fn launch(id: u32, launch: &TargetLaunch, tick: u64) -> Self {
        let (marked, skin) = launch.look();
        Self {
            id,
            row: launch.row,
            speed: launch.speed,
            marked,
            skin: TARGET_SKINS.iter().position(|s| *s == skin).unwrap_or(0) as u8,
            launched: tick,
            hit: false,
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=3).contains(&self.row)
                && (1..=5).contains(&self.speed)
                && usize::from(self.skin) < TARGET_SKINS.len(),
            "Invalid tutorial target"
        );
        Ok(())
    }
    /// Position of the target's origin at a (possibly fractional) tick.
    pub fn position(&self, tick: f64) -> Vec3 {
        let seconds = ((tick - self.launched as f64) / TICKS_PER_SECOND).max(0.0) as f32;
        let speed = TARGET_SPEEDS[usize::from(self.speed.clamp(1, 5) - 1)];
        Vec3::new(
            TARGET_START_X + speed * seconds,
            TARGET_Y,
            TARGET_LANES_Z[usize::from(self.row.clamp(1, 3) - 1)],
        )
    }
    /// `scrollTarget` has deleted it.
    pub fn gone(&self, tick: f64) -> bool {
        self.position(tick).x > TARGET_END_X
    }
    /// `setTransform(... eulerToQuat("0 0 -90"))`: a quarter turn about the
    /// up axis that points the board's painted face (the model's +x) back
    /// up the range at the shooters (+z).
    pub fn transform(&self, tick: f64) -> Mat4 {
        Mat4::from_rotation_translation(
            Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
            self.position(tick),
        )
    }
    /// The datablock's shape: standing, or the `...Hit` one once shot.
    pub fn shape(&self) -> &'static str {
        match (self.marked, self.hit) {
            (false, false) => TARGET_SHAPE,
            (false, true) => TARGET_HIT_SHAPE,
            (true, false) => TARGET_M_SHAPE,
            (true, true) => TARGET_M_HIT_SHAPE,
        }
    }
    pub fn skin(&self) -> &'static str {
        TARGET_SKINS[usize::from(self.skin).min(TARGET_SKINS.len() - 1)]
    }
}

/// What projectiles hit on a standing target: the `Collision-1` detail of
/// `target.dts` and `targetM.dts` in model space. The `...Hit` shapes have
/// no collision, so shots pass a target once it is down.
#[derive(Clone, Default)]
pub struct TargetCollision {
    pub plain: Vec<SharedShape>,
    pub marked: Vec<SharedShape>,
}
impl TargetCollision {
    pub fn of(&self, target: &TargetView) -> &[SharedShape] {
        match (target.hit, target.marked) {
            (true, _) => &[],
            (false, false) => &self.plain,
            (false, true) => &self.marked,
        }
    }
}

/// Read one of the pack's target shapes.
pub fn load_target_shape(dir: &Path, index: &PackIndex, id: &str) -> Result<Shape> {
    let file = index
        .shapes
        .get(id)
        .with_context(|| format!("Tutorial pack lacks {id}"))?;
    let path = dir.join(file);
    ensure!(
        std::fs::metadata(&path)
            .with_context(|| format!("Missing {}", path.display()))?
            .len()
            <= MAX_SHAPE_BYTES,
        "Tutorial target shape too large"
    );
    let shape: Shape =
        serde_json::from_slice(&std::fs::read(&path)?).with_context(|| format!("Reading {id}"))?;
    shape.validate()?;
    ensure!(shape.id == id, "Tutorial shape {file} is not {id}");
    Ok(shape)
}

/// The standing targets' collision, from the pack's shapes.
pub fn load_target_collision(dir: &Path, index: &PackIndex) -> Result<TargetCollision> {
    let collision = |id: &str| -> Result<Vec<SharedShape>> {
        let shape = load_target_shape(dir, index, id)?;
        let shapes: Vec<SharedShape> =
            bri_physics::content::static_shape_colliders(&shape, Mat4::IDENTITY)?
                .into_iter()
                .map(|builder| builder.shape)
                .collect();
        ensure!(!shapes.is_empty(), "{id} has no collision detail");
        Ok(shapes)
    };
    Ok(TargetCollision {
        plain: collision(TARGET_SHAPE)?,
        marked: collision(TARGET_M_SHAPE)?,
    })
}

/// Read and validate a tutorial pack's index.
pub fn load_index(dir: &Path) -> Result<PackIndex> {
    let path = dir.join(PACK_INDEX);
    ensure!(
        std::fs::metadata(&path)
            .with_context(|| format!("Missing {}", path.display()))?
            .len()
            <= MAX_INDEX_BYTES,
        "Tutorial pack index too large"
    );
    let index: PackIndex = serde_json::from_slice(&std::fs::read(&path)?)?;
    index.validate()?;
    Ok(index)
}

/// Read and validate a tutorial pack's index and both brick layouts.
pub fn load_pack(dir: &Path) -> Result<(PackIndex, World, World)> {
    let index = load_index(dir)?;
    let world = |file: &str| -> Result<World> {
        let world = bri_world::persistence::load(&dir.join(file))
            .with_context(|| format!("Loading tutorial world {file}"))?;
        ensure!(
            world.map_id == MAP_ID,
            "Tutorial world belongs to another map"
        );
        Ok(world)
    };
    let part1 = world(&index.part1)?;
    let part2 = world(&index.part2)?;
    Ok((index, part1, part2))
}
/// The trigger datablocks of `tutorial.cs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneKind {
    /// `TutorialWinTrigger`: completes its goal on entry.
    Win,
    /// `TutorialTipTrigger`: "Press <key> to <task>".
    Tip,
    Look,
    Brick,
    Build,
    Break,
    Ride,
    Water,
    Secret,
    Wrench,
    Print,
    Targets,
    Wand,
    Spray,
    JeepPark,
    Drive,
    FullWin,
}
impl ZoneKind {
    fn from_datablock(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().as_str() {
            "tutorialwintrigger" => Self::Win,
            "tutorialtiptrigger" => Self::Tip,
            "tutoriallooktiptrigger" => Self::Look,
            "tutorialbricktiptrigger" => Self::Brick,
            "tutorialbuildtiptrigger" => Self::Build,
            "tutorialbreaktiptrigger" => Self::Break,
            "tutorialridetiptrigger" => Self::Ride,
            "tutorialwatertrigger" => Self::Water,
            "tutorialsecrettiptrigger" => Self::Secret,
            "tutorialwrenchtiptrigger" => Self::Wrench,
            "tutorialprinttiptrigger" => Self::Print,
            "tutorialtargetstrigger" => Self::Targets,
            "tutorialwandtiptrigger" => Self::Wand,
            "tutorialspraytiptrigger" => Self::Spray,
            "tutorialjeepchecktrigger" => Self::JeepPark,
            "tutorialdrivetiptrigger" => Self::Drive,
            "tutorialfullwintrigger" => Self::FullWin,
            _ => return None,
        })
    }
}

/// A mission `Trigger` as a world-space box.
#[derive(Debug, Clone, PartialEq)]
pub struct Zone {
    pub kind: ZoneKind,
    /// `goalName`, `bindName` and `taskName` dynamic fields, empty if unset.
    pub goal: String,
    pub bind: String,
    pub task: String,
    pub min: Vec3,
    pub max: Vec3,
}
impl Zone {
    pub fn overlaps(&self, min: Vec3, max: Vec3) -> bool {
        self.min.cmple(max).all() && min.cmple(self.max).all()
    }
    pub fn contains(&self, point: Vec3) -> bool {
        self.min.cmple(point).all() && point.cmple(self.max).all()
    }
}

pub struct TutorialMap {
    pub zones: Vec<Zone>,
    /// `TutorialLookTarget`, the marker the Look lesson asks you to face.
    pub look_target: Vec3,
    pub part1: World,
    pub part2: World,
    pub targets: Vec<TargetLaunch>,
    pub targets_end_ms: u32,
    pub target_collision: TargetCollision,
}
impl TutorialMap {
    pub fn new(
        scene: &Scene,
        index: PackIndex,
        part1: World,
        part2: World,
        target_collision: TargetCollision,
    ) -> Result<Self> {
        index.validate()?;
        ensure!(
            scene.id == MAP_ID,
            "The tutorial rules belong to the Tutorial map"
        );
        let mut zones = Vec::new();
        let mut look_target = None;
        for node in &scene.nodes {
            if !matches!(node.kind, Kind::Unadapted) {
                continue;
            }
            let class = node.properties.get("source_class").map(String::as_str);
            if class == Some("marker") && node.name.eq_ignore_ascii_case("TutorialLookTarget") {
                look_target = Some(Vec3::new(
                    node.transform[12],
                    node.transform[13],
                    node.transform[14],
                ));
                continue;
            }
            if class != Some("trigger") {
                continue;
            }
            let datablock = node
                .properties
                .get("datablock")
                .context("Trigger without datablock")?;
            let kind = ZoneKind::from_datablock(datablock)
                .with_context(|| format!("Unknown tutorial trigger {datablock}"))?;
            // The stock polyhedron spans the unit box from the trigger's
            // corner; in native axes that is [0, 1] on every local axis.
            let transform = Mat4::from_cols_array(&node.transform);
            let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            for corner in 0..8 {
                let local = Vec3::new(
                    (corner & 1) as f32,
                    ((corner >> 1) & 1) as f32,
                    ((corner >> 2) & 1) as f32,
                );
                let point = transform.transform_point3(local);
                min = min.min(point);
                max = max.max(point);
            }
            ensure!(
                min.is_finite() && max.is_finite() && min.cmplt(max).all(),
                "Degenerate tutorial trigger"
            );
            let field = |key: &str| node.properties.get(key).cloned().unwrap_or_default();
            zones.push(Zone {
                kind,
                goal: field("goalname"),
                bind: field("bindname"),
                task: field("taskname"),
                min,
                max,
            });
        }
        for kind in [
            ZoneKind::Look,
            ZoneKind::FullWin,
            ZoneKind::JeepPark,
            ZoneKind::Drive,
        ] {
            ensure!(
                zones.iter().filter(|z| z.kind == kind).count() == 1,
                "Tutorial map needs exactly one {kind:?} trigger"
            );
        }
        ensure!(
            zones
                .iter()
                .all(|z| !matches!(z.kind, ZoneKind::Win | ZoneKind::Tip) || !z.goal.is_empty()),
            "Tutorial goal trigger without a goal name"
        );
        for world in [&part1, &part2] {
            world.validate()?;
            ensure!(
                world.map_id == MAP_ID,
                "Tutorial world belongs to another map"
            );
        }
        Ok(Self {
            zones,
            look_target: look_target.context("Tutorial map has no TutorialLookTarget")?,
            part1,
            part2,
            targets: index.targets,
            targets_end_ms: index.targets_end_ms,
            target_collision,
        })
    }
    pub fn zone(&self, kind: ZoneKind) -> Option<&Zone> {
        self.zones.iter().find(|z| z.kind == kind)
    }
}
