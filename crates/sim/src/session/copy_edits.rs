//! Changing builds for a duplicator Add-On, each as one step of the
//! player's undo: the bricks a copy was taken from painted (`paint_copy`)
//! or wrenched all at once (`wrench_copy`), a box cut out of builds with
//! plain bricks put back over what stuck out of it (`super_cut`), and a box
//! filled with plain bricks (`fill_box`). These are v20's New Duplicator's
//! fill colour, fill wrench, supercut and fill bricks; who may do which,
//! and when, is the Add-On's.
//!
//! Every brick changed needs the player's full trust (the spray can's and
//! the hammer's), as the New Duplicator asked; the rest are counted.
use super::*;
use crate::grid::Bounds;
use bri_package_runtime::ops::CopyPaint;
use bri_world::authority::trust as level;
use bri_world::{Emitter, Light};
mod box_jobs;
mod jobs;
use box_jobs::{FillWork, SuperCutWork};
pub(super) use jobs::{CutWork, PaintWork, WrenchWork};

/// A brick's paint: what a spray or FX can changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Look {
    pub color: u8,
    pub color_effect: u8,
    pub shape_effect: u8,
}
impl Look {
    pub fn of(brick: &Brick) -> Self {
        Self {
            color: brick.color,
            color_effect: brick.color_effect,
            shape_effect: brick.shape_effect,
        }
    }
    pub(super) fn put(self, brick: &mut Brick) {
        brick.color = self.color;
        brick.color_effect = self.color_effect;
        brick.shape_effect = self.shape_effect;
    }
    pub(super) fn painted(mut self, paint: CopyPaint) -> Self {
        match paint {
            CopyPaint::Color(c) => self.color = c,
            CopyPaint::ColorEffect(c) => self.color_effect = c,
            CopyPaint::ShapeEffect(c) => self.shape_effect = c,
        }
        self
    }
}

/// The settings ticked in the fill wrench: each one given is put on every
/// brick, the rest are left as each brick has them. `Some(None)` takes a
/// name, light, emitter or item away.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WrenchFill {
    #[serde(default)]
    pub name: Option<Option<String>>,
    #[serde(default)]
    pub light: Option<Option<String>>,
    #[serde(default)]
    pub emitter: Option<Option<String>>,
    /// 0 to 5, as the wrench's emitter direction buttons.
    #[serde(default)]
    pub emitter_direction: Option<u8>,
    #[serde(default)]
    pub item: Option<Option<String>>,
    /// 0 to 5, as the wrench's item position buttons.
    #[serde(default)]
    pub item_position: Option<u8>,
    /// 2 to 5, as the wrench's item direction buttons.
    #[serde(default)]
    pub item_direction: Option<u8>,
    #[serde(default)]
    pub item_respawn_ms: Option<u32>,
    #[serde(default)]
    pub raycast: Option<bool>,
    #[serde(default)]
    pub colliding: Option<bool>,
    #[serde(default)]
    pub visible: Option<bool>,
}
impl WrenchFill {
    /// Shape checks for a fill from the network.
    pub fn validate(&self) -> Result<()> {
        ensure!(*self != Self::default(), "Tick a setting to fill");
        let text = |t: &Option<Option<String>>| {
            t.iter()
                .flatten()
                .all(|s| !s.is_empty() && s.len() <= 512 && !s.chars().any(char::is_control))
        };
        ensure!(
            text(&self.name) && text(&self.light) && text(&self.emitter) && text(&self.item),
            "Invalid fill wrench setting"
        );
        ensure!(
            self.emitter_direction.is_none_or(|d| d <= 5)
                && self.item_position.is_none_or(|p| p <= 5)
                && self.item_direction.is_none_or(|d| (2..=5).contains(&d))
                && self.item_respawn_ms.is_none_or(|ms| ms <= 3_600_000),
            "Invalid fill wrench setting"
        );
        Ok(())
    }

    /// `brick` with the ticked settings put on it.
    fn apply(&self, brick: &Brick) -> Brick {
        let mut next = brick.clone();
        if let Some(name) = &self.name {
            next.name.clone_from(name);
        }
        if let Some(light) = &self.light {
            next.light = light.as_ref().map(|id| Light {
                asset: ContentRef::Resolved(id.clone()),
                enabled: true,
            });
        }
        let emitter = next.emitter.get_or_insert(Emitter {
            asset: None,
            direction: 0,
        });
        if let Some(asset) = &self.emitter {
            emitter.asset = asset.clone().map(ContentRef::Resolved);
        }
        if let Some(direction) = self.emitter_direction {
            emitter.direction = direction;
        }
        if self.emitter.is_none() && self.emitter_direction.is_none() {
            next.emitter.clone_from(&brick.emitter);
        }
        if let Some(item) = &self.item {
            next.item_spawn.item = item.clone().map(ContentRef::Resolved);
        }
        if let Some(position) = self.item_position {
            next.item_spawn.position = position;
        }
        if let Some(direction) = self.item_direction {
            next.item_spawn.direction = direction;
        }
        if let Some(ms) = self.item_respawn_ms {
            next.item_spawn.respawn_ms = ms;
        }
        if let Some(raycast) = self.raycast {
            next.raycast = raycast;
        }
        if let Some(colliding) = self.colliding {
            next.colliding = colliding;
        }
        if let Some(visible) = self.visible {
            next.visible = visible;
        }
        next
    }
}

/// The wrench settings `brick` has, as the wrench would send them; what
/// is unresolved here reads as none.
fn wrench_properties(brick: &Brick) -> WrenchProperties {
    let resolved = |r: &ContentRef| match r {
        ContentRef::Resolved(id) => Some(id.clone()),
        _ => None,
    };
    WrenchProperties {
        name: brick.name.clone(),
        light: brick.light.as_ref().and_then(|l| resolved(&l.asset)),
        emitter: brick
            .emitter
            .as_ref()
            .and_then(|e| e.asset.as_ref())
            .and_then(resolved),
        emitter_direction: brick.emitter.as_ref().map_or(0, |e| e.direction),
        item_spawn: brick.item_spawn.clone(),
        sound: None,
        vehicle: None,
        recolor_vehicle: false,
        raycast: brick.raycast,
        colliding: brick.colliding,
        visible: brick.visible,
    }
}

/// The wrench's part of `from` put on `onto`, for undo.
pub(super) fn wrenched_as(onto: &Brick, from: &Brick) -> Brick {
    let mut next = onto.clone();
    next.name.clone_from(&from.name);
    next.light.clone_from(&from.light);
    next.emitter.clone_from(&from.emitter);
    next.item_spawn = from.item_spawn.clone();
    next.raycast = from.raycast;
    next.colliding = from.colliding;
    next.visible = from.visible;
    next
}

/// How a supercut or fill went.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BoxEdit {
    /// Bricks removed (a supercut) or planted (a fill).
    pub bricks: usize,
    /// Plain bricks put back over what stuck out of the box.
    pub placed: usize,
    /// Bricks the player had no full trust on, left as they were; or, a
    /// fill, bricks that would not go in.
    pub refused: usize,
}

/// A plain brick the catalog has ([`plain_bricks`]): its id, and its grid
/// size unturned (studs along x, plates, studs along z).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Plain {
    id: String,
    size: [i32; 3],
}

/// Whether a brick is a plain box: v20's `BRICK` geometry, with studs on
/// top and nothing special about it.
fn is_plain(definition: &crate::definitions::Definition) -> bool {
    use bri_content::brick::Surface;
    let mesh = &definition.mesh;
    let [w, d] = mesh.footprint_studs;
    let h = mesh.height_plates;
    let full = [w as f32 * 0.5, h as f32 * 0.2, d as f32 * 0.5];
    let solid = (0..d).flat_map(|_| {
        (0..h).map(move |y| {
            let cell = if h == 1 {
                'b'
            } else if y == 0 {
                'u'
            } else if y == h - 1 {
                'd'
            } else {
                'x'
            };
            cell.to_string().repeat(w as usize)
        })
    });
    definition.special == crate::definitions::Special::None
        && !definition.indestructible
        && definition.reflection.is_none()
        && definition.link.is_none()
        && w > 0
        && d > 0
        && h > 0
        && mesh.collision_boxes.len() == 1
        && mesh.collision_boxes[0].center == [0.0; 3]
        && (0..3).all(|a| (mesh.collision_boxes[0].size[a] - full[a]).abs() < 1e-3)
        && mesh.attachment_rows.len() == (d * h) as usize
        && mesh.attachment_rows.iter().cloned().eq(solid)
        && !mesh
            .quads
            .iter()
            .any(|q| matches!(q.surface, Surface::Print | Surface::Ramp))
}

/// Each size of plain brick once (the first id of it), smallest first:
/// what a supercut and a fill build with (v20's New Duplicator's
/// `ndCreateSimpleBrickTable`).
fn plain_bricks(definitions: &crate::definitions::Definitions) -> Vec<Plain> {
    let mut seen = BTreeSet::new();
    let mut plain: Vec<Plain> = definitions
        .entries
        .iter()
        .filter(|(_, d)| is_plain(d))
        .filter_map(|(id, d)| {
            let [w, depth] = d.mesh.footprint_studs.map(|v| v as i32);
            let h = d.mesh.height_plates as i32;
            seen.insert((w.min(depth), w.max(depth), h)).then(|| Plain {
                id: id.clone(),
                size: [w, h, depth],
            })
        })
        .collect();
    plain.sort_by_key(|p| p.size.iter().product::<i32>());
    plain
}

/// Plain bricks filling `area` (grid cells), biggest first, as v20's New
/// Duplicator filled one (`ndFillAreaWithBricks`): the biggest brick that
/// fits goes in the low corner, long side along the area's long side, then
/// the room beside it, behind it and above it fill the same way. Each is
/// `template` there. Room no plain brick fits stays empty.
#[cfg(test)]
fn fill_cells(plain: &[Plain], area: Bounds, template: &Brick) -> Vec<Brick> {
    let mut out = Vec::new();
    let mut rooms = vec![area];
    while let Some(room) = rooms.pop() {
        if let Some((piece, _, more)) = fill_room(plain, room, template) {
            out.push(piece);
            rooms.extend(more);
        }
    }
    out
}

/// One step of [`fill_cells`]: the brick going in the low corner of
/// `room` with its size, and the rooms left beside it, behind it and above
/// it (to fill in the reverse order); `None` when no plain brick fits.
fn fill_room(
    plain: &[Plain],
    room: Bounds,
    template: &Brick,
) -> Option<(Brick, [i32; 3], [Bounds; 3])> {
    if room.size.iter().any(|&s| s <= 0) {
        return None;
    }
    let [rx, ry, rz] = room.size;
    // Long side along the room's long side, as the original turned
    // both to their sorted sizes.
    let turned = rx > rz;
    let (brick, size) = plain.iter().rev().find_map(|p| {
        let [w, h, d] = p.size;
        let (short, long) = (w.min(d), w.max(d));
        let size = if turned { [long, h, short] } else { [short, h, long] };
        let quarter = (w > d) != turned && w != d;
        (size[0] <= rx && size[1] <= ry && size[2] <= rz).then_some((
            (p.id.clone(), u8::from(quarter)),
            size,
        ))
    })?;
    let mut piece = template.clone();
    piece.definition = ContentRef::Resolved(brick.0);
    piece.quarter_turns = brick.1;
    piece.position = std::array::from_fn(|a| {
        (room.min[a] as f32 + size[a] as f32 * 0.5) * crate::grid::CELL[a]
    });
    let [x, y, z] = room.min;
    let [sx, sy, sz] = size;
    // Above it, behind it along z, beside it along x.
    let rooms = [
        Bounds {
            min: [x, y + sy, z],
            size: [sx, ry - sy, sz],
        },
        Bounds {
            min: [x, y, z + sz],
            size: [sx, ry, rz - sz],
        },
        Bounds {
            min: [x + sx, y, z],
            size: [rx - sx, ry, rz],
        },
    ];
    Some((piece, size, rooms))
}

/// The parts of `brick` (grid cells) lying outside `area`, as the
/// original cut them: the slabs past each end along x, then along z
/// within the box's x, then above and below within its x and z.
fn outside(brick: Bounds, area: Bounds) -> Vec<Bounds> {
    let (b0, b1, a0, a1) = (brick.min, brick.max(), area.min, area.max());
    let slab = |min: [i32; 3], max: [i32; 3]| Bounds {
        min,
        size: std::array::from_fn(|a| max[a] - min[a]),
    };
    let mut parts = Vec::new();
    if b0[0] < a0[0] {
        parts.push(slab(b0, [a0[0], b1[1], b1[2]]));
    }
    if b1[0] > a1[0] {
        parts.push(slab([a1[0], b0[1], b0[2]], b1));
    }
    let (x0, x1) = (b0[0].max(a0[0]), b1[0].min(a1[0]));
    if b0[2] < a0[2] {
        parts.push(slab([x0, b0[1], b0[2]], [x1, b1[1], a0[2]]));
    }
    if b1[2] > a1[2] {
        parts.push(slab([x0, b0[1], a1[2]], [x1, b1[1], b1[2]]));
    }
    let (z0, z1) = (b0[2].max(a0[2]), b1[2].min(a1[2]));
    if b0[1] < a0[1] {
        parts.push(slab([x0, b0[1], z0], [x1, a0[1], z1]));
    }
    if b1[1] > a1[1] {
        parts.push(slab([x0, a1[1], z0], [x1, b1[1], z1]));
    }
    parts.retain(|p| p.size.iter().all(|&s| s > 0));
    parts
}

impl Session {
    /// Paint the bricks `owner`'s copy was taken from in `color`, all or
    /// none, as one undo step. The number painted.
    pub fn paint_copy(&mut self, owner: OwnerId, color: u8) -> Result<usize> {
        self.paint_copy_with(owner, CopyPaint::Color(color), false)
            .map(|(painted, _)| painted)
    }

    /// Put `paint` on the bricks `owner`'s copy was taken from, as one undo
    /// step: with `each`, every brick they may paint (the number painted
    /// and the number refused); else all, or none when one is refused.
    /// All at once: an Add-On's paint is a job ([`Self::start_paint`]).
    pub fn paint_copy_with(
        &mut self,
        owner: OwnerId,
        paint: CopyPaint,
        each: bool,
    ) -> Result<(usize, usize)> {
        self.ensure_copy_idle(owner)?;
        let mut work = PaintWork::new(self, owner, paint, each)?;
        self.run_copy_work(owner, &mut work)?;
        work.complete(self, owner)
    }

    /// An Add-On's paint ([`Op::PaintCopy`]) as a copy job.
    pub(super) fn start_paint(&mut self, owner: OwnerId, package: &str, paint: CopyPaint, each: bool) {
        let started = self
            .ensure_copy_idle(owner)
            .and_then(|()| PaintWork::new(self, owner, paint, each));
        match started {
            Ok(work) => self.start_copy_job(owner, Some(package.into()), Box::new(work)),
            Err(error) => self.report_paint(package, owner, each, Err(error)),
        }
    }

    /// Tell `package` (or else `player`) how a paint went: painting all
    /// or none, the player hears it whatever the Add-On.
    pub(super) fn report_paint(
        &mut self,
        package: &str,
        player: OwnerId,
        each: bool,
        result: Result<(usize, usize)>,
    ) {
        if !each {
            match result {
                Ok((count, _)) => self.bottom_count(player, "Painted", count),
                Err(error) => self.center_print(player, format!("{error:#}")),
            }
            return;
        }
        let outcome = match result {
            Ok((bricks, refused)) => {
                let mut outcome = copy_store::CopyOutcome::about("paint", None, None);
                outcome.bricks = bricks;
                outcome.total = bricks + refused;
                outcome.refused = refused;
                outcome
            }
            Err(error) => copy_store::CopyOutcome::failed(
                "paint",
                self.blueprints.contains_key(&player),
                error,
            ),
        };
        self.report_copy(package, player, outcome);
    }

    /// Open `owner`'s wrench on every brick their copy was taken from; what
    /// they tick comes back to [`Self::wrench_copy`].
    pub fn open_copy_wrench(&mut self, owner: OwnerId) -> Result<()> {
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        combat::ensure_may_build(
            &peer.combat,
            &self.minigames,
            bri_minigames::BuildAction::Build,
        )?;
        self.ensure_copy_idle(owner)?;
        let held = self.copies.get_mut(&owner).context("Copy a build first")?;
        held.wrench_open = true;
        let ids = held.sources.clone();
        let bricks = ids.len() as u32;
        self.unlight_bricks(ids)?;
        self.notify(owner, Notice::WrenchCopy { bricks });
        Ok(())
    }

    /// Put the fill wrench's ticked settings on each brick `owner`'s copy
    /// was taken from that they may change, as one undo step, a slice a
    /// tick: the number changed and the number refused, when done at
    /// once. Only once per opening.
    pub fn wrench_copy(&mut self, owner: OwnerId, fill: &WrenchFill) -> Result<Option<(usize, usize)>> {
        self.ensure_copy_idle(owner)?;
        let work = WrenchWork::new(self, owner, fill.clone())?;
        let package = self.copies.get(&owner).map(|c| c.package.clone());
        Ok(self
            .begin_copy_job(owner, package, work)?
            .map(|work| work.complete(self, owner, true)))
    }

    /// Remove `ids`, as a cut does, once read.
    pub(super) fn cut_out_unread(&mut self, ids: &[BrickId]) -> Result<()> {
        for &id in ids {
            self.highlight_forget(id);
        }
        // Checked by the caller; the engine removes them in one pass.
        let engine = Actor {
            administrator: true,
            ..Default::default()
        };
        self.simulation.remove_many(&engine, ids)?;
        for &id in ids {
            self.dirty.insert(id);
            self.events.respawns.remove(&id);
            self.close_inspections(id);
        }
        Ok(())
    }

    /// v20's New Duplicator's supercut: every brick reaching into the box
    /// from `min` to `max` (world units, grown to the grid) that `owner`
    /// may hammer goes, and plain bricks in its colours, as its owner's,
    /// fill what stuck out of the box. Water bricks stay. One undo step,
    /// `package`'s. All at once: an Add-On's supercut is a job
    /// ([`Self::start_super_cut`]).
    pub fn super_cut(
        &mut self,
        owner: OwnerId,
        min: [f32; 3],
        max: [f32; 3],
        package: Option<&str>,
    ) -> Result<BoxEdit> {
        self.ensure_copy_idle(owner)?;
        let mut work = SuperCutWork::new(self, owner, (min, max), package)?;
        let done = self.run_copy_work(owner, &mut work);
        let edit = work.complete(self, owner);
        done.map(|()| edit)
    }

    /// [`Self::super_cut`] as a copy job, `package` told how it went.
    pub(super) fn start_super_cut(&mut self, owner: OwnerId, package: &str, (min, max): ([f32; 3], [f32; 3])) {
        let started = self
            .ensure_copy_idle(owner)
            .and_then(|()| SuperCutWork::new(self, owner, (min, max), Some(package)));
        match started {
            Ok(work) => self.start_copy_job(owner, Some(package.into()), Box::new(work)),
            Err(error) => self.report_box_edit(package, owner, "supercut", Err(error)),
        }
    }

    /// Fill the box from `min` to `max` with plain bricks of palette colour
    /// `color` as `owner`'s own, biggest first, as v20's New Duplicator's
    /// `/fillBricks` did; a brick that would not go in is left out, and
    /// the fill stops at the server's brick limit. One undo step,
    /// `package`'s. All at once: an Add-On's fill is a job
    /// ([`Self::start_fill`]).
    pub fn fill_box(
        &mut self,
        owner: OwnerId,
        (min, max): ([f32; 3], [f32; 3]),
        color: u8,
        package: Option<&str>,
    ) -> Result<BoxEdit> {
        self.ensure_copy_idle(owner)?;
        let mut work = FillWork::new(self, owner, (min, max), color, package)?;
        let done = self.run_copy_work(owner, &mut work);
        let edit = work.complete(self, owner);
        done.map(|()| edit)
    }

    /// [`Self::fill_box`] as a copy job, `package` told how it went.
    pub(super) fn start_fill(
        &mut self,
        owner: OwnerId,
        package: &str,
        (min, max): ([f32; 3], [f32; 3]),
        color: u8,
    ) {
        let started = self
            .ensure_copy_idle(owner)
            .and_then(|()| FillWork::new(self, owner, (min, max), color, Some(package)));
        match started {
            Ok(work) => self.start_copy_job(owner, Some(package.into()), Box::new(work)),
            Err(error) => self.report_box_edit(package, owner, "fill", Err(error)),
        }
    }

    /// Tell `package` (or else `player`) how a supercut or fill went.
    pub(super) fn report_box_edit(
        &mut self,
        package: &str,
        player: OwnerId,
        action: &'static str,
        result: Result<BoxEdit>,
    ) {
        let outcome = match result {
            Ok(edit) => copy_store::CopyOutcome {
                working: false,
                names: Vec::new(),
                action,
                name: None,
                bricks: edit.bricks,
                placed: edit.placed,
                total: edit.bricks + edit.refused,
                limit_reached: false,
                refused: edit.refused,
                error: None,
            },
            Err(error) => copy_store::CopyOutcome::failed(action, true, error),
        };
        self.report_copy(package, player, outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(id: &str, size: [i32; 3]) -> Plain {
        Plain {
            id: id.into(),
            size,
        }
    }

    #[test]
    fn a_fill_takes_the_biggest_bricks_first_and_leaves_no_gap() {
        let table = vec![
            plain("1x1f", [1, 1, 1]),
            plain("1x1", [1, 3, 1]),
            plain("2x4", [2, 3, 4]),
        ];
        let area = Bounds {
            min: [0, 0, 0],
            size: [5, 4, 4],
        };
        let template = Brick::new(ContentRef::Resolved(String::new()), [0.0; 3], 7);
        let bricks = fill_cells(&table, area, &template);
        // Two 2x4s, then 1x1s and 1x1 plates for the rest.
        let count = |id: &str| {
            bricks
                .iter()
                .filter(|b| b.definition == ContentRef::Resolved(id.into()))
                .count()
        };
        assert_eq!(count("2x4"), 2);
        let cells: i32 = bricks
            .iter()
            .map(|b| {
                let p = table
                    .iter()
                    .find(|p| b.definition == ContentRef::Resolved(p.id.clone()))
                    .unwrap();
                p.size.iter().product::<i32>()
            })
            .sum();
        assert_eq!(cells, 5 * 4 * 4, "the box is full");
        assert!(bricks.iter().all(|b| b.owner == 7));
    }

    #[test]
    fn what_sticks_out_of_a_box_is_cut_into_slabs_outside_it() {
        let brick = Bounds {
            min: [-2, 0, 0],
            size: [6, 3, 2],
        };
        let area = Bounds {
            min: [0, 1, 0],
            size: [2, 4, 4],
        };
        let parts = outside(brick, area);
        let cells: i32 = parts.iter().map(|p| p.size.iter().product::<i32>()).sum();
        // 6x3x2 = 36 cells, 2x2x2 = 8 of them inside.
        assert_eq!(cells, 36 - 8);
        for part in &parts {
            let (p0, p1) = (part.min, part.max());
            let inside = (0..3).all(|a| p0[a] >= area.min[a] && p1[a] <= area.max()[a]);
            assert!(!inside);
        }
    }
}
