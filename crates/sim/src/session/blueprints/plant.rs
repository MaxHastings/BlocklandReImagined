//! Planting a copy as a copy job, a slice of bricks each tick: all of it
//! or none (each brick checked first, then planted), or, for a copy that
//! plants each brick that fits ([`HeldCopy::partial`]), brick by brick as
//! v20's duplicators planted.
use super::*;
use crate::blueprint::{Inexact, Placement};
use crate::session::copy_jobs::{CopyWork, Ending, Progress};
use crate::simulation::{PlantFailure, Support, spend, work};

/// Why bricks of a plant were refused: how many for each plant error, and
/// the first.
#[derive(Default)]
pub(in crate::session) struct Refusals {
    pub first: Option<anyhow::Error>,
    pub count: usize,
    pub by_error: BTreeMap<&'static str, usize>,
}
impl Refusals {
    pub fn add(&mut self, error: anyhow::Error) {
        self.add_many(error, 1);
    }
    fn add_many(&mut self, error: anyhow::Error, n: usize) {
        if n == 0 {
            return;
        }
        self.count += n;
        *self
            .by_error
            .entry(crate::session::packages::copy_hooks::plant_error(&error))
            .or_default() += n;
        self.first.get_or_insert(error);
    }
    /// One refusal for everything (a plant refused before any brick).
    pub fn all(error: anyhow::Error) -> Self {
        let mut refusals = Self::default();
        refusals.add(error);
        refusals
    }
}

enum Phase {
    /// All or none: every brick checked against the world as it stands,
    /// and whether the world holds any of them up. `lowest` becomes a
    /// baseplate when none is held up and the plant may float.
    Check {
        next: usize,
        supported: bool,
        lowest: (usize, f32),
    },
    /// All or none, checked: each planted. `base` is the baseplate.
    Place { next: usize, base: Option<usize> },
    /// Something got in the way part way: what went in comes out.
    Undo { error: Option<anyhow::Error> },
    /// Brick by brick. A brick with nothing under it yet waits for the
    /// next pass, as long as a pass plants something.
    Each {
        waiting: Vec<u32>,
        next: usize,
        floating: Vec<u32>,
        planted_before: usize,
    },
}

/// A copy going into the world.
pub(in crate::session) struct PlantWork {
    copy: Arc<Blueprint>,
    placement: Placement,
    inexact: Inexact,
    actor: Actor,
    package: Option<String>,
    anchor: [f32; 3],
    support: Support,
    phase: Phase,
    ids: Vec<BrickId>,
    refused: Refusals,
}

impl PlantWork {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        copy: Arc<Blueprint>,
        placement: Placement,
        inexact: Inexact,
        actor: Actor,
        package: Option<String>,
        anchor: [f32; 3],
        (partial, float): (bool, bool),
    ) -> Self {
        let support = if float { Support::Float } else { Support::Required };
        let phase = if partial {
            Phase::Each {
                waiting: (0..copy.len() as u32).collect(),
                next: 0,
                floating: Vec::new(),
                planted_before: 0,
            }
        } else {
            Phase::Check {
                next: 0,
                supported: false,
                lowest: (0, f32::INFINITY),
            }
        };
        Self {
            ids: Vec::with_capacity(copy.len()),
            copy,
            placement,
            inexact,
            actor,
            package,
            anchor,
            support,
            phase,
            refused: Refusals::default(),
        }
    }

    fn work(&mut self, s: &mut Session, owner: OwnerId, budget: &mut u32) -> Result<bool> {
        loop {
            match &mut self.phase {
                Phase::Check {
                    next,
                    supported,
                    lowest,
                } => {
                    while *next < self.copy.len() {
                        if !spend(budget, work::PLANT) {
                            return Ok(false);
                        }
                        let brick = self.placement.brick(&self.copy, &self.copy.bricks[*next]);
                        match s.simulation.check_plant(&self.actor, &brick) {
                            Ok(held) => *supported |= held,
                            Err(error) => {
                                self.refused = Refusals::all(error);
                                return Ok(true);
                            }
                        }
                        if brick.position[1] < lowest.1 {
                            *lowest = (*next, brick.position[1]);
                        }
                        *next += 1;
                    }
                    let base = match (*supported, self.support) {
                        (true, _) => None,
                        (false, Support::Float) => Some(lowest.0),
                        (false, _) => {
                            self.refused = Refusals::all(PlantFailure::Float.into());
                            return Ok(true);
                        }
                    };
                    self.phase = Phase::Place { next: 0, base };
                }
                Phase::Place { next, base } => {
                    let base = *base;
                    while *next < self.copy.len() {
                        if !spend(budget, work::PLANT) {
                            return Ok(false);
                        }
                        let i = *next;
                        *next += 1;
                        let mut brick = self.placement.brick(&self.copy, &self.copy.bricks[i]);
                        brick.base_plate |= base == Some(i);
                        match s.simulation.plant_try(&self.actor, brick, true) {
                            Ok(id) => {
                                self.ids.push(id);
                                s.special_planted(owner, id)?;
                                s.dirty.insert(id);
                            }
                            Err(error) => {
                                self.phase = Phase::Undo { error: Some(error) };
                                break;
                            }
                        }
                    }
                    if matches!(self.phase, Phase::Place { .. }) {
                        return Ok(true);
                    }
                }
                Phase::Undo { error } => {
                    let engine = Actor {
                        administrator: true,
                        ..Default::default()
                    };
                    while let Some(&id) = self.ids.last() {
                        if !spend(budget, work::REMOVE) {
                            return Ok(false);
                        }
                        self.ids.pop();
                        if s.simulation.state().bricks.contains_key(&id) {
                            s.simulation.remove(&engine, id)?;
                            s.dirty.insert(id);
                        }
                    }
                    let error = error.take().expect("taken once");
                    self.refused = Refusals::all(error);
                    return Ok(true);
                }
                Phase::Each {
                    waiting,
                    next,
                    floating,
                    planted_before,
                } => {
                    let free = self.support == Support::Free;
                    while let Some(&i) = waiting.get(*next) {
                        if !spend(budget, work::PLANT) {
                            return Ok(false);
                        }
                        *next += 1;
                        let brick = self.placement.brick(&self.copy, &self.copy.bricks[i as usize]);
                        match s.simulation.plant_try(&self.actor, brick, free) {
                            Ok(id) => {
                                self.ids.push(id);
                                s.special_planted(owner, id)?;
                                s.dirty.insert(id);
                            }
                            Err(error) => {
                                if matches!(error.downcast_ref(), Some(PlantFailure::Float)) {
                                    floating.push(i);
                                } else {
                                    self.refused.add(error);
                                }
                            }
                        }
                    }
                    if floating.is_empty() {
                        return Ok(true);
                    }
                    if self.ids.len() == *planted_before {
                        if self.support != Support::Float {
                            self.refused.add_many(PlantFailure::Float.into(), floating.len());
                            return Ok(true);
                        }
                        // Nothing of the copy holds the rest up: the lowest
                        // becomes ground for whatever stands on it.
                        let copy = &self.copy;
                        let lowest = (0..floating.len())
                            .min_by(|&a, &b| {
                                let y = |k: usize| copy.bricks[floating[k] as usize].position[1];
                                y(a).total_cmp(&y(b))
                            })
                            .expect("not empty");
                        let i = floating.swap_remove(lowest);
                        let mut base = self.placement.brick(&self.copy, &self.copy.bricks[i as usize]);
                        base.base_plate = true;
                        match s.simulation.plant_try(&self.actor, base, true) {
                            Ok(id) => {
                                self.ids.push(id);
                                s.special_planted(owner, id)?;
                                s.dirty.insert(id);
                            }
                            Err(error) => self.refused.add(error),
                        }
                    }
                    *waiting = std::mem::take(floating);
                    *next = 0;
                    *planted_before = self.ids.len();
                }
            }
        }
    }

    /// Wrap up what went in: one undo step, the player's plant rate and
    /// wait used up, and the Add-On told. The first brick planted, or why
    /// none was.
    pub fn complete(self, s: &mut Session, owner: OwnerId, canceled: bool) -> Result<Reply> {
        let tick = s.simulation.state().tick;
        if let Some(package) = &self.package {
            s.report_place(
                package,
                owner,
                (self.ids.len(), self.copy.len(), canceled),
                &self.refused,
                &self.inexact,
            );
        }
        let Some(&first) = self.ids.first() else {
            return Err(match self.refused.first {
                Some(error) => error,
                None => anyhow::anyhow!("Nothing to plant"),
            });
        };
        let rate = s.admin.settings.bricks_per_second;
        if let Some(peer) = s.peers.get_mut(&owner) {
            peer.plants = peer.plants.max(rate);
        }
        if let Some(wait) = s.plant_waits.get_mut(&owner) {
            wait.next = tick + wait.ticks;
        }
        let entry = undo::UndoEntry::Group {
            ids: self.ids,
            group: self.actor.owner,
        };
        s.push_copy_undo(owner, entry, self.package);
        s.cues
            .emit(tick, crate::presentation::CueKind::Plant, self.anchor);
        s.play_thread_three(tick, owner, "plant");
        Ok(Reply::Planted(first))
    }
}

impl CopyWork for PlantWork {
    fn progress(&self) -> Progress {
        let total = self.copy.len();
        let done = match &self.phase {
            Phase::Check { next, .. } => next / 2,
            Phase::Place { next, .. } => (total + next) / 2,
            _ => self.ids.len() + self.refused.count,
        };
        Progress {
            action: "plant",
            done: done.min(total),
            total,
        }
    }

    fn step(&mut self, s: &mut Session, owner: OwnerId, budget: &mut u32) -> Result<bool> {
        let before = self.ids.len();
        let done = self.work(s, owner, budget);
        if self.ids.len() != before {
            s.simulation.settle();
        }
        done
    }

    fn finish(self: Box<Self>, s: &mut Session, owner: OwnerId, ending: Ending) {
        let canceled = match ending {
            Ending::Done | Ending::Left => false,
            Ending::Canceled => true,
            Ending::Failed(error) => {
                let mut work = *self;
                work.refused.add(error);
                let _ = work.complete(s, owner, false);
                return;
            }
        };
        let package = self.package.is_some();
        if let Err(error) = self.complete(s, owner, canceled)
            && !package
            && !canceled
        {
            s.center_print(owner, format!("{error:#}"));
        }
    }
}
