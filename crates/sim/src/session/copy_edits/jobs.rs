//! Cutting, painting and wrenching the bricks a copy was taken from as
//! copy jobs, a slice each tick. Cutting and painting all or none first
//! check every brick, then change them; a brick that went meanwhile is
//! passed over, and one whose trust went is left as it is.
use super::*;
use crate::session::copy_jobs::{CopyWork, Ending, Progress};
use crate::simulation::{spend, work};
use std::sync::Arc;

/// A player changing the bricks a copy took, with the trust they had.
struct Editor {
    actor: Actor,
    /// The copy's rule lets full trust in a brick's stack owner do
    /// ([`crate::simulation::Simulation::stack_owner`], `CopyRule::stack`).
    stack: bool,
}
impl Editor {
    fn of(peer: &Peer, held: &crate::session::blueprints::HeldCopy) -> Self {
        Self {
            actor: peer.actor.clone(),
            stack: held.stack,
        }
    }
    /// Whether they may change `brick` (`id`): full trust in its owner,
    /// or, as the copy's rule allows, in the owner of its stack.
    fn may(&self, s: &Session, id: BrickId, brick: &Brick) -> bool {
        self.actor.trusted(brick.owner, level::FULL)
            || (self.stack
                && s.simulation
                    .stack_owner(id)
                    .is_some_and(|o| o != 0 && self.actor.trust_level(o) >= level::FULL))
    }
}
impl std::ops::Deref for Editor {
    type Target = Actor;
    fn deref(&self) -> &Actor {
        &self.actor
    }
}

/// Checking that every brick of a copy still standing may be changed
/// with full trust, a slice at a time.
#[derive(Default)]
struct TrustCheck {
    next: usize,
    standing: usize,
    refused: usize,
}
impl TrustCheck {
    /// True once done; an error when a brick is refused or none stands.
    fn step(
        &mut self,
        s: &Session,
        actor: &Editor,
        ids: &[BrickId],
        budget: &mut u32,
    ) -> Result<bool> {
        let world = s.simulation.state();
        while let Some(id) = ids.get(self.next) {
            if !spend(budget, work::EDIT) {
                return Ok(false);
            }
            self.next += 1;
            if let Some(brick) = world.bricks.get(id) {
                self.standing += 1;
                if !actor.may(s, *id, brick) {
                    self.refused += 1;
                }
            }
        }
        ensure!(
            self.standing > 0,
            "The bricks this copy was taken from are gone"
        );
        ensure!(
            self.refused == 0,
            "{} of these bricks belong to builds that do not trust you enough.",
            self.refused
        );
        Ok(true)
    }
}

/// Cutting away the bricks a copy was taken from ([`Session::cut_copy`]):
/// all or none, or with `each` every brick the player may cut, the rest
/// counted.
pub(in crate::session) struct CutWork {
    ids: Arc<Vec<BrickId>>,
    actor: Editor,
    package: Option<String>,
    check: TrustCheck,
    next: usize,
    removed: Vec<(BrickId, Brick)>,
    refused: usize,
    sum: Vec3,
}
impl CutWork {
    pub fn new(s: &Session, owner: OwnerId, each: bool) -> Result<Self> {
        let peer = s.peers.get(&owner).context("Unknown connection")?;
        combat::ensure_may_build(
            &peer.combat,
            &s.minigames,
            bri_minigames::BuildAction::Build,
        )?;
        let held = s.copies.get(&owner).context("Copy a build first")?;
        // Cutting each that may be: no check of all first.
        let check = TrustCheck {
            next: if each { held.sources.len() } else { 0 },
            standing: usize::from(each),
            refused: 0,
        };
        Ok(Self {
            ids: held.sources.clone(),
            actor: Editor::of(peer, held),
            package: Some(held.package.clone()),
            check,
            next: 0,
            // Room for all of them now, not a copy of all so far as it grows.
            removed: Vec::with_capacity(held.sources.len()),
            refused: 0,
            sum: Vec3::ZERO,
        })
    }

    /// One undo step that puts the cut bricks back; how many were cut.
    pub fn complete(self, s: &mut Session, owner: OwnerId) -> usize {
        let count = self.removed.len();
        if count > 0 {
            let middle = self.sum / count as f32;
            s.push_copy_undo(owner, undo::UndoEntry::Cut(self.removed), self.package);
            let tick = s.simulation.state().tick;
            s.cues
                .emit(tick, crate::presentation::CueKind::Plant, middle.to_array());
        }
        count
    }
}
impl CopyWork for CutWork {
    fn progress(&self) -> Progress {
        Progress {
            action: "cut",
            done: (self.check.next + self.next) / 2,
            total: self.ids.len(),
            placed: 0,
            refused: 0,
            ..Default::default()
        }
    }
    fn step(&mut self, s: &mut Session, owner: OwnerId, budget: &mut u32) -> Result<bool> {
        self.actor.actor = s.live_copy_actor(owner, Some(bri_minigames::BuildAction::Build))?;
        if !self.check.step(s, &self.actor, &self.ids, budget)? {
            return Ok(false);
        }
        let mut slice = Vec::new();
        while let Some(&id) = self.ids.get(self.next) {
            if !spend(budget, work::REMOVE) {
                break;
            }
            self.next += 1;
            let Some(brick) = s.simulation.state().bricks.get(&id) else {
                continue;
            };
            if !self.actor.may(s, id, brick) {
                self.refused += 1;
                continue;
            }
            self.sum += Vec3::from(brick.position);
            self.removed.push((id, s.unlit(id, brick)));
            slice.push(id);
            s.simulation.mark_rebuild(id);
            s.simulation.charge_rebuilds(budget);
        }
        if !slice.is_empty() {
            s.cut_out_unread(&slice)?;
        }
        Ok(self.next == self.ids.len())
    }
    fn finish(self: Box<Self>, s: &mut Session, owner: OwnerId, ending: Ending) {
        let package = self.package.clone().unwrap_or_default();
        let outcome = match ending {
            Ending::Failed(error) if self.removed.is_empty() => {
                copy_store::CopyOutcome::failed("cut", true, error)
            }
            Ending::Left => {
                self.complete(s, owner);
                return;
            }
            ending => {
                let refused = self.refused;
                let bricks = self.complete(s, owner);
                // What was cut before a cancel stays cut, one undo step.
                let canceled = matches!(ending, Ending::Canceled)
                    .then(|| ("canceled", "Cut canceled!".to_string()));
                let mut outcome = copy_store::CopyOutcome::about("cut", None, canceled);
                outcome.bricks = bricks;
                outcome.refused = refused;
                outcome.total = bricks + refused;
                outcome
            }
        };
        s.report_copy(&package, owner, outcome);
    }
}

/// Painting the bricks a copy was taken from ([`Session::paint_copy_with`]).
pub(in crate::session) struct PaintWork {
    ids: Arc<Vec<BrickId>>,
    actor: Editor,
    package: String,
    paint: FillPaint,
    each: bool,
    check: TrustCheck,
    next: usize,
    painted: usize,
    refused: usize,
    before: Vec<(BrickId, Look)>,
}
impl PaintWork {
    pub fn new(s: &Session, owner: OwnerId, paint: FillPaint, each: bool) -> Result<Self> {
        let peer = s.peers.get(&owner).context("Unknown connection")?;
        combat::ensure_may_build(
            &peer.combat,
            &s.minigames,
            bri_minigames::BuildAction::Paint,
        )?;
        match paint {
            FillPaint::Color(color) => ensure!(
                usize::from(color) < s.simulation.state().palette.len(),
                "That colour is not in this server's palette"
            ),
            FillPaint::ColorEffect(fx) => ensure!(fx <= 6, "Unknown colour effect"),
            FillPaint::ShapeEffect(fx) => ensure!(fx <= 2, "Unknown shape effect"),
        }
        let held = s.copies.get(&owner).context("Copy a build first")?;
        // Painting each that may be: only none standing refuses it.
        let check = TrustCheck {
            next: if each { held.sources.len() } else { 0 },
            standing: usize::from(each),
            refused: 0,
        };
        Ok(Self {
            ids: held.sources.clone(),
            actor: Editor::of(peer, held),
            package: held.package.clone(),
            paint,
            each,
            check,
            next: 0,
            painted: 0,
            refused: 0,
            before: Vec::with_capacity(held.sources.len()),
        })
    }

    /// One undo step that puts the old paint back; how many were painted
    /// and refused.
    pub fn complete(self, s: &mut Session, owner: OwnerId) -> Result<(usize, usize)> {
        ensure!(
            self.painted + self.refused > 0,
            "The bricks this copy was taken from are gone"
        );
        if !self.before.is_empty() {
            s.push_copy_undo(
                owner,
                undo::UndoEntry::Looks(self.before),
                Some(self.package),
            );
        }
        Ok((self.painted, self.refused))
    }
}
impl CopyWork for PaintWork {
    fn progress(&self) -> Progress {
        let total = self.ids.len();
        let done = if self.each {
            self.next
        } else {
            (self.check.next + self.next) / 2
        };
        Progress {
            action: "paint",
            done,
            total,
            placed: 0,
            refused: 0,
            ..Default::default()
        }
    }
    fn step(&mut self, s: &mut Session, owner: OwnerId, budget: &mut u32) -> Result<bool> {
        self.actor.actor = s.live_copy_actor(owner, Some(bri_minigames::BuildAction::Paint))?;
        if !self.check.step(s, &self.actor, &self.ids, budget)? {
            return Ok(false);
        }
        let mut changed = Vec::new();
        while let Some(&id) = self.ids.get(self.next) {
            if !spend(budget, work::EDIT) {
                break;
            }
            self.next += 1;
            let Some(brick) = s.simulation.state().bricks.get(&id) else {
                continue;
            };
            if !self.actor.may(s, id, brick) {
                self.refused += 1;
                continue;
            }
            self.painted += 1;
            // Painted, a brick stops glowing.
            s.unlight_one(id)?;
            let brick = &s.simulation.state().bricks[&id];
            let look = Look::of(brick);
            let painted = look.painted(self.paint);
            if painted != look {
                let mut next = brick.clone();
                painted.put(&mut next);
                self.before.push((id, look));
                changed.push((id, next));
            }
        }
        if !changed.is_empty() {
            s.dirty.extend(changed.iter().map(|(id, _)| *id));
            s.simulation.replace_many(changed)?;
        }
        Ok(self.next == self.ids.len())
    }
    fn finish(self: Box<Self>, s: &mut Session, owner: OwnerId, ending: Ending) {
        let (package, each) = (self.package.clone(), self.each);
        let result = match ending {
            Ending::Left => {
                let _ = self.complete(s, owner);
                return;
            }
            Ending::Failed(error) if self.before.is_empty() => Err(error),
            Ending::Canceled => {
                let result = self.complete(s, owner);
                s.report_paint_canceled(&package, owner, each, result);
                return;
            }
            _ => self.complete(s, owner),
        };
        s.report_paint(&package, owner, each, result);
    }
}

/// Putting the fill wrench's settings on the bricks a copy was taken from
/// ([`Session::wrench_copy`]).
pub(in crate::session) struct WrenchWork {
    ids: Arc<Vec<BrickId>>,
    actor: Editor,
    package: String,
    fill: WrenchFill,
    next: usize,
    count: usize,
    refused: usize,
    before: Vec<(BrickId, Brick)>,
}
impl WrenchWork {
    pub fn new(s: &mut Session, owner: OwnerId, fill: WrenchFill) -> Result<Self> {
        fill.validate()?;
        let held = s.copies.get_mut(&owner).context("Copy a build first")?;
        ensure!(
            std::mem::take(&mut held.wrench_open),
            "Open the fill wrench first"
        );
        let (ids, package, stack) = (held.sources.clone(), held.package.clone(), held.stack);
        let peer = s.peers.get(&owner).context("Unknown connection")?;
        combat::ensure_may_build(
            &peer.combat,
            &s.minigames,
            bri_minigames::BuildAction::Build,
        )?;
        Ok(Self {
            before: Vec::with_capacity(ids.len()),
            ids,
            actor: Editor {
                actor: peer.actor.clone(),
                stack,
            },
            package,
            fill,
            next: 0,
            count: 0,
            refused: 0,
        })
    }

    /// One undo step that puts the old settings back, and the Add-On told
    /// (that it was canceled, with `canceled`).
    pub fn complete(
        self,
        s: &mut Session,
        owner: OwnerId,
        report: bool,
        canceled: bool,
    ) -> (usize, usize) {
        let (count, refused) = (self.count, self.refused);
        if !self.before.is_empty() {
            let entry = undo::UndoEntry::Wrenched(self.before);
            s.push_copy_undo(owner, entry, Some(self.package.clone()));
        }
        if report {
            let canceled = canceled.then(|| ("canceled", "Fill wrench canceled!".to_string()));
            let mut outcome = copy_store::CopyOutcome::about("wrench", None, canceled);
            outcome.bricks = count;
            outcome.total = count + refused;
            outcome.refused = refused;
            s.report_copy(&self.package, owner, outcome);
        }
        (count, refused)
    }
}
impl CopyWork for WrenchWork {
    fn progress(&self) -> Progress {
        Progress {
            action: "wrench",
            done: self.next,
            total: self.ids.len(),
            placed: 0,
            refused: 0,
            ..Default::default()
        }
    }
    fn step(&mut self, s: &mut Session, owner: OwnerId, budget: &mut u32) -> Result<bool> {
        self.actor.actor = s.live_copy_actor(owner, Some(bri_minigames::BuildAction::Build))?;
        let tick = s.simulation.state().tick;
        let mut changed = Vec::new();
        let mut stocked = Vec::new();
        while let Some(&id) = self.ids.get(self.next) {
            if !spend(budget, work::EDIT * 2) {
                break;
            }
            self.next += 1;
            let Some(brick) = s.simulation.state().bricks.get(&id) else {
                continue;
            };
            if !self.actor.may(s, id, brick) {
                self.refused += 1;
                continue;
            }
            s.unlight_one(id)?;
            let brick = &s.simulation.state().bricks[&id];
            let mut next = self.fill.apply(brick);
            // The server's light, emitter and item limits, as the wrench.
            let mut properties = wrench_properties(&next);
            s.quota_wrench(brick, &mut properties);
            if properties.light.is_none() && brick.light.is_none() {
                next.light = None;
            }
            if properties.emitter.is_none()
                && let Some(emitter) = &mut next.emitter
                && brick.emitter.as_ref().is_none_or(|e| e.asset.is_none())
            {
                emitter.asset = None;
            }
            if properties.item_spawn.item.is_none() && brick.item_spawn.item.is_none() {
                next.item_spawn.item = None;
            }
            let edit = Edit::Properties(wrench_properties(&next));
            if s.tool_catalog.validate_edit(brick, &edit).is_err()
                || s.item_spawners
                    .validate_edit(s.simulation.state(), id, &edit)
                    .is_err()
            {
                self.refused += 1;
                continue;
            }
            if next == *brick {
                continue;
            }
            if next.item_spawn.item.is_some() && next.item_spawn != brick.item_spawn {
                stocked.push(id);
            }
            let collision = next.colliding != brick.colliding;
            self.before.push((id, brick.clone()));
            changed.push((id, next));
            self.count += 1;
            if collision {
                s.simulation.mark_rebuild(id);
                s.simulation.charge_rebuilds(budget);
            }
        }
        if !changed.is_empty() {
            s.dirty.extend(changed.iter().map(|(id, _)| *id));
            s.simulation.replace_many(changed)?;
            for id in stocked {
                s.item_spawners.restock(id, tick);
            }
        }
        Ok(self.next == self.ids.len())
    }
    fn finish(self: Box<Self>, s: &mut Session, owner: OwnerId, ending: Ending) {
        let report = !matches!(ending, Ending::Left);
        self.complete(s, owner, report, matches!(ending, Ending::Canceled));
    }
}
