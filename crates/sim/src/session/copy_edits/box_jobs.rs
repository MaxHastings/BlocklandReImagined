//! A supercut and a fill as copy jobs, a slice each tick, as v20's New
//! Duplicator cut a chunk of the box every 30 ms (`tickSuperCutChunk`).
//! Cancelled part way, what was cut or filled stays, as one undo step.
use super::*;
use crate::session::copy_jobs::{CopyWork, Ending, Progress};
use crate::simulation::{BoxScan, spend, work};
use bri_package_runtime::ops::MAX_COPY_BRICKS;

/// The engine's actor planting `owner`'s bricks for them, whether or not
/// anything holds them up.
fn planter(owner: OwnerId) -> Actor {
    Actor {
        owner,
        administrator: true,
        ..Default::default()
    }
}

/// The middle of `area`, in world units.
fn middle(area: Bounds) -> [f32; 3] {
    std::array::from_fn(|a| (area.min[a] as f32 + area.size[a] as f32 * 0.5) * crate::grid::CELL[a])
}

/// A supercut ([`Session::super_cut`]): the box searched, then each brick
/// reaching into it that the player may hammer cut, and the plain bricks
/// in its colours put back over what stuck out, brick by brick.
pub(in crate::session) struct SuperCutWork {
    area: Bounds,
    actor: Actor,
    package: Option<String>,
    scan: BoxScan,
    plain: Vec<Plain>,
    found: Vec<BrickId>,
    next: usize,
    /// The plain bricks for the slice being cut.
    pieces: Vec<Brick>,
    removed: Vec<(BrickId, Brick)>,
    placed: Vec<BrickId>,
    refused: usize,
}
impl SuperCutWork {
    pub fn new(s: &Session, owner: OwnerId, (min, max): ([f32; 3], [f32; 3]), package: Option<&str>) -> Result<Self> {
        let area = blueprints::grid_box(min, max)?;
        let peer = s.peers.get(&owner).context("Unknown connection")?;
        combat::ensure_may_build(&peer.combat, &s.minigames, bri_minigames::BuildAction::Build)?;
        Ok(Self {
            area,
            actor: peer.actor.clone(),
            package: package.map(str::to_string),
            scan: BoxScan::new(&s.simulation, area, false, MAX_COPY_BRICKS as usize),
            plain: plain_bricks(&s.simulation.definitions),
            found: Vec::new(),
            next: 0,
            pieces: Vec::new(),
            removed: Vec::new(),
            placed: Vec::new(),
            refused: 0,
        })
    }

    /// One undo step for what was done; how it went.
    pub fn complete(self, s: &mut Session, owner: OwnerId) -> BoxEdit {
        let edit = BoxEdit {
            bricks: self.removed.len(),
            placed: self.placed.len(),
            refused: self.refused,
        };
        if !self.removed.is_empty() {
            let tick = s.simulation.state().tick;
            s.cues
                .emit(tick, crate::presentation::CueKind::Plant, middle(self.area));
            let entry = undo::UndoEntry::Replaced {
                removed: self.removed,
                placed: self.placed,
            };
            s.push_copy_undo(owner, entry, self.package);
        }
        edit
    }

    fn work(&mut self, s: &mut Session, budget: &mut u32) -> Result<bool> {
        if !self.scan.is_done() {
            if !self.scan.step(&s.simulation, budget, |_, _| true) {
                return Ok(false);
            }
            ensure!(
                !self.scan.selection.limit_reached,
                "That box holds more than {MAX_COPY_BRICKS} bricks; supercut a smaller one."
            );
            self.found = std::mem::take(&mut self.scan.selection.bricks);
            self.removed.reserve(self.found.len());
        }
        // A slice of the bricks read and their plain bricks worked out, then
        // all of them cut at once, then the plain bricks in.
        let mut slice = Vec::new();
        while let Some(&id) = self.found.get(self.next) {
            if !spend(budget, work::REMOVE + work::EDIT) {
                break;
            }
            self.next += 1;
            let world = s.simulation.state();
            let Some(brick) = world.bricks.get(&id) else {
                continue;
            };
            // Water bricks stay.
            let water = s
                .simulation
                .definitions
                .get(brick)
                .is_ok_and(|d| d.special == crate::definitions::Special::Water);
            if water {
                continue;
            }
            if !self.actor.trusted(brick.owner, level::FULL) {
                self.refused += 1;
                continue;
            }
            let brick = s.unlit(id, brick);
            let bounds = s.simulation.index_bounds(id);
            let mut template = Brick::new(ContentRef::Resolved(String::new()), [0.0; 3], brick.owner);
            Look::of(&brick).put(&mut template);
            template.raycast = brick.raycast;
            template.colliding = brick.colliding;
            template.visible = brick.visible;
            for part in outside(bounds, self.area) {
                let mut rooms = vec![part];
                while let Some(room) = rooms.pop() {
                    if let Some((piece, _, more)) = fill_room(&self.plain, room, &template) {
                        // Paid for now: it goes in with this slice.
                        *budget = budget.saturating_sub(work::PLANT);
                        self.pieces.push(piece);
                        rooms.extend(more);
                    }
                }
            }
            self.removed.push((id, brick));
            slice.push(id);
            s.simulation.mark_rebuild(id);
            s.simulation.charge_rebuilds(budget);
        }
        if !slice.is_empty() {
            s.cut_out_unread(&slice)?;
        }
        for piece in std::mem::take(&mut self.pieces) {
            // One that will not go in is left out, as the original's.
            if let Ok(id) = s.simulation.plant_try(&planter(piece.owner), piece, true) {
                self.placed.push(id);
                s.dirty.insert(id);
            }
            s.simulation.charge_rebuilds(budget);
        }
        Ok(self.next == self.found.len())
    }
}
impl CopyWork for SuperCutWork {
    fn progress(&self) -> Progress {
        Progress {
            action: "supercut",
            done: self.next,
            total: self.found.len(),
            placed: self.placed.len(),
            refused: self.refused,
            ..Default::default()
        }
    }
    fn step(&mut self, s: &mut Session, _: OwnerId, budget: &mut u32) -> Result<bool> {
        let placed = self.placed.len();
        let done = self.work(s, budget);
        if self.placed.len() != placed {
            s.simulation.settle();
        }
        done
    }
    fn finish(self: Box<Self>, s: &mut Session, owner: OwnerId, ending: Ending) {
        let package = self.package.clone().unwrap_or_default();
        match ending {
            Ending::Failed(error) if self.removed.is_empty() => {
                s.report_box_edit(&package, owner, "supercut", Err(error));
            }
            Ending::Left => {
                self.complete(s, owner);
            }
            Ending::Canceled => {
                let edit = self.complete(s, owner);
                report(s, &package, owner, "supercut", &edit, Some("Supercut canceled!"));
            }
            _ => {
                let edit = self.complete(s, owner);
                report(s, &package, owner, "supercut", &edit, None);
            }
        }
    }
}

/// A fill ([`Session::fill_box`]): plain bricks of one colour, the room
/// left in the box filled a brick at a time, biggest first.
pub(in crate::session) struct FillWork {
    area: Bounds,
    actor: Actor,
    package: Option<String>,
    plain: Vec<Plain>,
    template: Brick,
    rooms: Vec<Bounds>,
    ids: Vec<BrickId>,
    refused: usize,
    /// Grid cells of the box gone through, of all of them.
    covered: u64,
    volume: u64,
    limit_reached: bool,
}
impl FillWork {
    pub fn new(
        s: &Session,
        owner: OwnerId,
        (min, max): ([f32; 3], [f32; 3]),
        color: u8,
        package: Option<&str>,
    ) -> Result<Self> {
        let area = blueprints::grid_box(min, max)?;
        let peer = s.peers.get(&owner).context("Unknown connection")?;
        combat::ensure_may_build(&peer.combat, &s.minigames, bri_minigames::BuildAction::Build)?;
        ensure!(
            usize::from(color) < s.simulation.state().palette.len(),
            "That colour is not in this server's palette"
        );
        let mut template = Brick::new(ContentRef::Resolved(String::new()), [0.0; 3], owner);
        template.color = color;
        Ok(Self {
            area,
            actor: peer.actor.clone(),
            package: package.map(str::to_string),
            plain: plain_bricks(&s.simulation.definitions),
            template,
            rooms: vec![area],
            ids: Vec::new(),
            refused: 0,
            covered: 0,
            volume: volume(area),
            limit_reached: false,
        })
    }

    /// One undo step for what went in; how it went.
    pub fn complete(self, s: &mut Session, owner: OwnerId) -> BoxEdit {
        let edit = BoxEdit {
            bricks: self.ids.len(),
            placed: 0,
            refused: self.refused,
        };
        if !self.ids.is_empty() {
            let tick = s.simulation.state().tick;
            s.cues
                .emit(tick, crate::presentation::CueKind::Plant, middle(self.area));
            let entry = undo::UndoEntry::Group {
                ids: self.ids,
                group: self.actor.owner,
            };
            s.push_copy_undo(owner, entry, self.package);
        }
        edit
    }

    fn work(&mut self, s: &mut Session, budget: &mut u32) -> bool {
        let limit = (s.admin.settings.brick_limit as usize).min(bri_world::MAX_BRICKS);
        while let Some(room) = self.rooms.pop() {
            if !spend(budget, work::PLANT) {
                self.rooms.push(room);
                return false;
            }
            let Some((piece, size, more)) = fill_room(&self.plain, room, &self.template) else {
                self.covered += volume(room);
                continue;
            };
            if s.simulation.state().bricks.len() >= limit || self.ids.len() >= MAX_COPY_BRICKS as usize {
                self.limit_reached = true;
                return true;
            }
            self.covered += volume(Bounds { min: room.min, size });
            self.rooms.extend(more);
            match s.simulation.plant_try(&self.actor, piece, true) {
                Ok(id) => {
                    self.ids.push(id);
                    s.dirty.insert(id);
                }
                Err(_) => self.refused += 1,
            }
            s.simulation.charge_rebuilds(budget);
        }
        true
    }
}
impl CopyWork for FillWork {
    fn progress(&self) -> Progress {
        Progress {
            action: "fill",
            done: self.covered as usize,
            total: self.volume as usize,
            placed: 0,
            refused: self.refused,
            ..Default::default()
        }
    }
    fn step(&mut self, s: &mut Session, _: OwnerId, budget: &mut u32) -> Result<bool> {
        let planted = self.ids.len();
        let done = self.work(s, budget);
        if self.ids.len() != planted {
            s.simulation.settle();
        }
        Ok(done)
    }
    fn finish(self: Box<Self>, s: &mut Session, owner: OwnerId, ending: Ending) {
        let package = self.package.clone().unwrap_or_default();
        let limit_reached = self.limit_reached;
        match ending {
            Ending::Failed(error) if self.ids.is_empty() => {
                s.report_box_edit(&package, owner, "fill", Err(error));
            }
            Ending::Left => {
                self.complete(s, owner);
            }
            Ending::Canceled => {
                let edit = self.complete(s, owner);
                report(s, &package, owner, "fill", &edit, Some("Fill canceled!"));
            }
            _ if limit_reached && self.ids.is_empty() => {
                let error = anyhow::anyhow!("That would pass the server's brick limit.");
                s.report_box_edit(&package, owner, "fill", Err(error));
            }
            _ => {
                let edit = self.complete(s, owner);
                let mut outcome = outcome("fill", &edit, None);
                outcome.limit_reached = limit_reached;
                s.report_copy(&package, owner, outcome);
            }
        }
    }
}

/// What a supercut or fill did, for its Add-On: `canceled` with the
/// words for it when the player stopped it.
fn outcome(action: &'static str, edit: &BoxEdit, canceled: Option<&str>) -> copy_store::CopyOutcome {
    let error = canceled.map(|message| ("canceled", message.to_string()));
    let mut outcome = copy_store::CopyOutcome::about(action, None, error);
    outcome.bricks = edit.bricks;
    outcome.placed = edit.placed;
    outcome.total = edit.bricks + edit.refused;
    outcome.refused = edit.refused;
    outcome
}

fn report(
    s: &mut Session,
    package: &str,
    owner: OwnerId,
    action: &'static str,
    edit: &BoxEdit,
    canceled: Option<&str>,
) {
    let outcome = outcome(action, edit, canceled);
    s.report_copy(package, owner, outcome);
}

fn volume(area: Bounds) -> u64 {
    area.size.iter().map(|&s| s.max(0) as u64).product()
}
