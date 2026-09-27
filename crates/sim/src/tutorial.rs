//! Native content for the v20 Tutorial map: the mission's trigger zones, read
//! from the converted scene, and the tutorial pack converted from
//! `Map_Tutorial` (its brick layouts and target practice schedule). The rules
//! that use them live in `session::tutorial`.
use anyhow::{Context, Result, ensure};
use bri_content::scene::{Kind, Scene};
pub use bri_content::tutorial::{PACK_INDEX, PackIndex, TargetLaunch};
use bri_world::World;
use glam::{Mat4, Vec3};
use std::path::Path;

pub const MAP_ID: &str = "v20/add-ons/map_tutorial/tutorial.mis";
const MAX_INDEX_BYTES: u64 = 1 << 20;

/// Read and validate a tutorial pack's index and both brick layouts.
pub fn load_pack(dir: &Path) -> Result<(PackIndex, World, World)> {
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
}
impl TutorialMap {
    pub fn new(scene: &Scene, index: PackIndex, part1: World, part2: World) -> Result<Self> {
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
        })
    }
    pub fn zone(&self, kind: ZoneKind) -> Option<&Zone> {
        self.zones.iter().find(|z| z.kind == kind)
    }
}
