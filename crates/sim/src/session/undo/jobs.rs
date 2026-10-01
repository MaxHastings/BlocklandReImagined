//! Undoing a big copy step as a copy job, a slice each tick: the bricks a
//! plant put in broken, the bricks a cut took put back, paint and wrench
//! settings put back. Cancelled part way, what is undone stays undone and
//! the rest stays on the undo stack.
use super::*;
use crate::session::copy_edits::Look;
use crate::session::copy_jobs::{CopyWork, Ending, Progress};
use crate::simulation::{spend, work};
use crate::id_map::IdSet;

/// A big undo done over several ticks: its Add-On hears it is finished.
fn report_undone(s: &mut Session, owner: OwnerId, by: Option<&str>, bricks: usize) {
    if let Some(package) = by {
        let mut outcome = copy_store::CopyOutcome::about("undone", None, None);
        outcome.bricks = bricks;
        outcome.total = bricks;
        s.report_copy(package, owner, outcome);
    }
}

/// The bricks of a placed copy broken, last placed first
/// ([`Session::undo_group`]).
pub(in crate::session) struct UndoGroup {
    ids: Vec<BrickId>,
    group: OwnerId,
    by: Option<String>,
    actor: Actor,
    /// The copy's bricks, gathered first.
    copy: IdSet,
    gathered: usize,
    /// Bricks of `ids` still to break: those before this.
    left: usize,
    first: Option<BrickId>,
}
impl UndoGroup {
    pub fn new(s: &Session, owner: OwnerId, ids: Vec<BrickId>, group: OwnerId, by: Option<String>) -> Result<Self> {
        let actor = s.peers.get(&owner).context("Unknown connection")?.actor.clone();
        Ok(Self {
            left: ids.len(),
            copy: IdSet::default(),
            gathered: 0,
            ids,
            group,
            by,
            actor,
            first: None,
        })
    }
    pub fn complete(self) -> Reply {
        Reply::Undone(self.first)
    }
}
impl UndoGroup {
    /// Break the copy's bricks, last placed first, as far as `budget`
    /// allows.
    fn break_slice(&mut self, s: &mut Session, owner: OwnerId, budget: &mut u32) -> Result<()> {
        // Bricks joined only to the copy break together.
        let mut quiet: Vec<BrickId> = Vec::new();
        while self.left > 0 {
            if !spend(budget, work::BREAK) {
                break;
            }
            self.left -= 1;
            let id = self.ids[self.left];
            let Some(brick) = s.simulation.state().bricks.get(&id) else {
                continue;
            };
            if brick.owner != self.group {
                continue;
            }
            let outside: Vec<OwnerId> = s
                .simulation
                .connected_bricks(id)?
                .into_iter()
                .filter(|n| !self.copy.contains(*n))
                .map(|n| s.simulation.state().bricks[&n].owner)
                .collect();
            if outside.is_empty() {
                quiet.push(id);
                s.close_inspections(id);
                s.simulation.mark_rebuild(id);
            } else {
                *budget = budget.saturating_sub(work::CHAIN);
                if !quiet.is_empty() {
                    s.kill_bricks(&self.actor, &std::mem::take(&mut quiet))?;
                }
                // Joined to bricks outside the copy: broken as one undone
                // plant is (`killBrick`, its chain kill and
                // `undoTrustCheck`).
                let untrusting = outside
                    .into_iter()
                    .find(|&group| self.actor.trust_level(group) < level::FULL);
                if let Some(group) = untrusting
                    && s.simulation.will_cause_chain_kill(id)?
                {
                    let name = s.brick_group_name(group);
                    s.center_print(owner, format!("{name} does not trust you enough to do that."));
                    continue;
                }
                s.tool_kill_brick(owner, id)?;
            }
            s.simulation.charge_rebuilds(budget);
            self.first.get_or_insert(id);
        }
        if !quiet.is_empty() {
            s.kill_bricks(&self.actor, &quiet)?;
        }
        Ok(())
    }
}

impl CopyWork for UndoGroup {
    fn progress(&self) -> Progress {
        let total = self.ids.len();
        Progress {
            action: "undo",
            done: total - self.left,
            total,
            placed: 0,
            refused: 0,
        }
    }
    fn step(&mut self, s: &mut Session, owner: OwnerId, budget: &mut u32) -> Result<bool> {
        while let Some(&id) = self.ids.get(self.gathered) {
            if !spend(budget, work::SCAN) {
                return Ok(false);
            }
            self.copy.insert(id);
            self.gathered += 1;
        }
        // One collision refresh for the slice, however its bricks break.
        s.simulation.hold_settle(true);
        let broken = self.break_slice(s, owner, budget);
        s.simulation.hold_settle(false);
        broken?;
        Ok(self.left == 0)
    }
    fn finish(mut self: Box<Self>, s: &mut Session, owner: OwnerId, ending: Ending) {
        if let Ending::Failed(error) = &ending {
            s.center_print(owner, format!("{error:#}"));
        }
        if matches!(ending, Ending::Done) {
            report_undone(s, owner, self.by.as_deref(), self.ids.len());
        }
        if !matches!(ending, Ending::Done) && self.left > 0 {
            self.ids.truncate(self.left);
            let entry = UndoEntry::Group {
                ids: std::mem::take(&mut self.ids),
                group: self.group,
            };
            s.push_copy_undo(owner, entry, self.by.take());
        }
    }
}

/// The bricks a cut took put back as they were, all or none
/// ([`Session::undo_cut`]): each checked, then each put back, then the
/// owner's undo steps and held copy follow them to their new ids. Undoing
/// a supercut, the plain bricks it put in come out first, and go back in
/// when something else stands in the way.
pub(in crate::session) struct UndoCut {
    bricks: Vec<(BrickId, Brick)>,
    by: Option<String>,
    /// A supercut's plain bricks, and how many have been gone through.
    pieces: Option<Vec<BrickId>>,
    cleared: usize,
    /// Those of them taken out, as they were, and put back in when blocked.
    taken: Vec<Brick>,
    retaken: Vec<BrickId>,
    checked: usize,
    /// Each brick put back, in order: its new id.
    back: Vec<BrickId>,
    /// Old ids to new of the bricks put back.
    renamed: Renamed,
    /// Old ids to new, following once every brick is back.
    follow: Option<Follow>,
    /// Something got in the way: the bricks put back come out again.
    blocked: Option<anyhow::Error>,
}
impl UndoCut {
    pub fn new(bricks: Vec<(BrickId, Brick)>, by: Option<String>) -> Self {
        Self {
            back: Vec::with_capacity(bricks.len()),
            renamed: Renamed::default(),
            bricks,
            by,
            pieces: None,
            cleared: 0,
            taken: Vec::new(),
            retaken: Vec::new(),
            checked: 0,
            follow: None,
            blocked: None,
        }
    }
    /// Undoing a supercut: `placed` out, then `removed` back.
    pub fn replaced(removed: Vec<(BrickId, Brick)>, placed: Vec<BrickId>, by: Option<String>) -> Self {
        Self {
            pieces: Some(placed),
            ..Self::new(removed, by)
        }
    }
    /// The step left to undo: `bricks` still to put back, with a
    /// supercut's plain bricks still standing over them.
    fn rest(&mut self, bricks: Vec<(BrickId, Brick)>) -> UndoEntry {
        match self.pieces.take() {
            Some(pieces) => {
                let mut placed = std::mem::take(&mut self.retaken);
                placed.extend_from_slice(&pieces[self.cleared..]);
                UndoEntry::Replaced {
                    removed: bricks,
                    placed,
                }
            }
            None => UndoEntry::Cut(bricks),
        }
    }
    /// Blocked, keep the step to try again.
    pub fn complete(mut self, s: &mut Session, owner: OwnerId) -> Reply {
        if let Some(error) = self.blocked.take() {
            let text = match error.downcast_ref::<crate::simulation::PlantFailure>() {
                Some(_) => "Something is in the way of the bricks you cut.".to_string(),
                None => format!("{error:#}"),
            };
            s.center_print(owner, text);
            let bricks = std::mem::take(&mut self.bricks);
            let entry = self.rest(bricks);
            s.push_copy_undo(owner, entry, self.by);
            return Reply::Undone(None);
        }
        crate::drop_later::drop_later(self.bricks);
        Reply::Undone(self.back.first().copied())
    }

    /// A supercut's plain bricks taken out, as far as `budget` allows.
    fn clear(&mut self, s: &mut Session, budget: &mut u32) -> Result<bool> {
        let Some(pieces) = &self.pieces else {
            return Ok(true);
        };
        let mut slice = Vec::new();
        while let Some(&id) = pieces.get(self.cleared) {
            if !spend(budget, work::REMOVE) {
                break;
            }
            self.cleared += 1;
            if let Some(brick) = s.simulation.state().bricks.get(&id) {
                self.taken.push(s.unlit(id, brick));
                slice.push(id);
                s.simulation.mark_rebuild(id);
                s.simulation.charge_rebuilds(budget);
            }
        }
        if !slice.is_empty() {
            s.cut_out_unread(&slice)?;
        }
        Ok(self.cleared == pieces.len())
    }

    /// Blocked and the bricks put back out again: the plain bricks taken
    /// out go back in, as far as `budget` allows.
    fn unclear(&mut self, s: &mut Session, budget: &mut u32) -> bool {
        let before = self.retaken.len();
        while let Some(brick) = self.taken.pop() {
            if !spend(budget, work::RESTORE) {
                self.taken.push(brick);
                break;
            }
            if let Ok(id) = s.simulation.restore_one(brick) {
                self.retaken.push(id);
                s.dirty.insert(id);
            }
            s.simulation.charge_rebuilds(budget);
        }
        if self.retaken.len() != before {
            s.simulation.settle();
        }
        self.taken.is_empty()
    }
}
impl CopyWork for UndoCut {
    fn progress(&self) -> Progress {
        Progress {
            action: "undo",
            done: (self.checked + self.back.len()) / 2,
            total: self.bricks.len(),
            placed: 0,
            refused: 0,
        }
    }
    fn step(&mut self, s: &mut Session, owner: OwnerId, budget: &mut u32) -> Result<bool> {
        if let Some(follow) = &mut self.follow {
            return Ok(s.follow_some(owner, follow, budget));
        }
        if self.blocked.is_none() && !self.clear(s, budget)? {
            return Ok(false);
        }
        if self.blocked.is_none() {
            while let Some((_, brick)) = self.bricks.get(self.checked) {
                if !spend(budget, work::PLANT) {
                    return Ok(false);
                }
                self.checked += 1;
                if !s.simulation.fits(brick) {
                    self.blocked = Some(crate::simulation::PlantFailure::Overlap.into());
                    return Ok(self.unclear(s, budget));
                }
            }
            let before = self.back.len();
            while let Some((old, brick)) = self.bricks.get(self.back.len()) {
                if !spend(budget, work::RESTORE) {
                    break;
                }
                match s.simulation.restore_one(brick.clone()) {
                    Ok(id) => {
                        self.back.push(id);
                        self.renamed.insert(*old, id);
                        s.dirty.insert(id);
                    }
                    Err(error) => {
                        self.blocked = Some(error);
                        break;
                    }
                }
                s.simulation.charge_rebuilds(budget);
            }
            if self.back.len() != before {
                s.simulation.settle();
            }
            if self.blocked.is_none() {
                if self.back.len() < self.bricks.len() {
                    return Ok(false);
                }
                // All back: follow them to their new ids with what is left.
                let renamed = std::mem::take(&mut self.renamed);
                let follow = self.follow.insert(Follow::new(s, owner, renamed));
                return Ok(s.follow_some(owner, follow, budget));
            }
        }
        // Take back what went in.
        let engine = Actor {
            administrator: true,
            ..Default::default()
        };
        let mut slice = Vec::new();
        while let Some(&id) = self.back.last() {
            if !spend(budget, work::REMOVE) {
                break;
            }
            self.back.pop();
            if s.simulation.state().bricks.contains_key(&id) {
                slice.push(id);
                s.simulation.mark_rebuild(id);
                s.simulation.charge_rebuilds(budget);
            }
        }
        if !slice.is_empty() {
            s.simulation.remove_many(&engine, &slice)?;
            s.dirty.extend(slice);
        }
        Ok(self.back.is_empty() && self.unclear(s, budget))
    }
    fn finish(mut self: Box<Self>, s: &mut Session, owner: OwnerId, ending: Ending) {
        if let Ending::Failed(error) = &ending {
            s.center_print(owner, format!("{error:#}"));
        }
        let blocked = self.blocked.is_some();
        if matches!(ending, Ending::Done) && blocked {
            self.complete(s, owner);
            return;
        }
        // The bricks back so far stay back, under their new ids; the rest
        // stays to undo.
        let mut follow = match self.follow.take() {
            Some(follow) => follow,
            None => Follow::new(s, owner, std::mem::take(&mut self.renamed)),
        };
        s.follow_all(owner, &mut follow);
        let rest = self.bricks.split_off(self.back.len());
        let (by, bricks) = (self.by.clone(), self.bricks.len());
        // A supercut's plain bricks stand over the rest only while none
        // went back.
        let rest = match (rest.is_empty(), self.back.is_empty()) {
            (true, _) => None,
            (false, true) => Some(self.rest(rest)),
            (false, false) => Some(UndoEntry::Cut(rest)),
        };
        // Its plain bricks taken out stay out.
        drop(std::mem::take(&mut self.taken));
        self.blocked = None;
        self.complete(s, owner);
        if let Some(rest) = rest {
            s.push_copy_undo(owner, rest, by);
        } else if matches!(ending, Ending::Done) {
            report_undone(s, owner, by.as_deref(), bricks);
        }
    }
}

/// Paint or wrench settings put back on the bricks still standing that the
/// undoer may change ([`Session::undo_edits`]).
pub(in crate::session) struct UndoEdits {
    edits: Edits,
    actor: Actor,
    by: Option<String>,
    next: usize,
    first: Option<BrickId>,
}
pub(in crate::session) enum Edits {
    Looks(Vec<(BrickId, Look)>),
    Wrenched(Vec<(BrickId, Brick)>),
}
impl UndoEdits {
    pub fn new(s: &Session, owner: OwnerId, edits: Edits, by: Option<String>) -> Result<Self> {
        let actor = s.peers.get(&owner).context("Unknown connection")?.actor.clone();
        Ok(Self {
            edits,
            actor,
            by,
            next: 0,
            first: None,
        })
    }
    pub fn complete(self) -> Reply {
        Reply::Undone(self.first)
    }
    fn len(&self) -> usize {
        match &self.edits {
            Edits::Looks(looks) => looks.len(),
            Edits::Wrenched(bricks) => bricks.len(),
        }
    }
}
impl CopyWork for UndoEdits {
    fn progress(&self) -> Progress {
        Progress {
            action: "undo",
            done: self.next,
            total: self.len(),
            placed: 0,
            refused: 0,
        }
    }
    fn step(&mut self, s: &mut Session, _: OwnerId, budget: &mut u32) -> Result<bool> {
        let palette = s.simulation.state().palette.len();
        let mut restored = Vec::new();
        while self.next < self.len() {
            if !spend(budget, work::EDIT) {
                break;
            }
            let i = self.next;
            self.next += 1;
            let id = match &self.edits {
                Edits::Looks(looks) => looks[i].0,
                Edits::Wrenched(bricks) => bricks[i].0,
            };
            let Some(now) = s.simulation.state().bricks.get(&id) else {
                continue;
            };
            if !self.actor.trusted(now.owner, level::FULL) {
                continue;
            }
            let next = match &self.edits {
                Edits::Looks(looks) => {
                    if usize::from(looks[i].1.color) >= palette {
                        continue;
                    }
                    let mut next = now.clone();
                    looks[i].1.put(&mut next);
                    next
                }
                Edits::Wrenched(bricks) => copy_edits::wrenched_as(now, &bricks[i].1),
            };
            let collision = next.colliding != now.colliding;
            self.first.get_or_insert(id);
            restored.push((id, next));
            if collision {
                s.simulation.mark_rebuild(id);
                s.simulation.charge_rebuilds(budget);
            }
        }
        if !restored.is_empty() {
            s.dirty.extend(restored.iter().map(|(id, _)| *id));
            s.simulation.replace_many(restored)?;
        }
        Ok(self.next == self.len())
    }
    fn finish(self: Box<Self>, s: &mut Session, owner: OwnerId, ending: Ending) {
        if let Ending::Failed(error) = &ending {
            s.center_print(owner, format!("{error:#}"));
        }
        if matches!(ending, Ending::Done) {
            report_undone(s, owner, self.by.as_deref(), self.len());
            return;
        }
        let (next, by) = (self.next, self.by);
        let entry = match self.edits {
            Edits::Looks(mut looks) => UndoEntry::Looks(looks.split_off(next.min(looks.len()))),
            Edits::Wrenched(mut bricks) => {
                UndoEntry::Wrenched(bricks.split_off(next.min(bricks.len())))
            }
        };
        if entry.bricks() > 0 {
            s.push_copy_undo(owner, entry, by);
        }
    }
}
