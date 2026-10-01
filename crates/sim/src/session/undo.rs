//! v20's per-client undo stack (`%client.undoStack`) and `serverCmdUndoBrick`.
//!
//! One mixed stack records plants, spray paint, FX paint and prints. Each
//! Ctrl+Z pops exactly one entry: an entry whose brick is gone is spent
//! without effect, as in v20. Undoing a plant is a `killBrick`, so the brick
//! breaks with the hammer's sound and debris.
use super::*;
use bri_world::authority::trust as level;
use crate::simulation::{spend, work};
mod jobs;

/// game.cs: `%client.undoStack = New_QueueSO(512)`. `QueueSO` keeps one slot
/// empty to tell a full ring from an empty one, so it holds 511 entries.
pub const UNDO_QUEUE_SIZE: usize = 512;

/// One `undoStack` line: the brick and what to put back.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum UndoEntry {
    /// `PLANT`
    Plant(BrickId),
    /// A placed copy (`place_blueprint`), planted into `group`'s bricks:
    /// one Ctrl+Z takes it all back.
    Group { ids: Vec<BrickId>, group: OwnerId },
    /// Bricks cut away (`cut_copy`), with the ids they had, as they were:
    /// one Ctrl+Z puts them all back.
    Cut(Vec<(BrickId, Brick)>),
    /// Bricks painted together (`paint_copy`), with the paint each had.
    Looks(Vec<(BrickId, copy_edits::Look)>),
    /// Bricks wrenched together (`wrench_copy`), as each was.
    Wrenched(Vec<(BrickId, Brick)>),
    /// A supercut: the bricks cut, as they were, and the plain bricks put
    /// back over what stuck out of the box.
    Replaced {
        removed: Vec<(BrickId, Brick)>,
        placed: Vec<BrickId>,
    },
    /// `COLOR`, from the colour spray cans.
    Color(BrickId, u8),
    /// `COLORFX`, from the colour FX cans.
    ColorEffect(BrickId, u8),
    /// `SHAPEFX`, from the shape FX cans.
    ShapeEffect(BrickId, u8),
    /// `PRINT`, from `serverCmdSetPrint`.
    Print(BrickId, Option<ContentRef>),
}
impl UndoEntry {
    fn brick(&self) -> BrickId {
        match *self {
            Self::Group { ref ids, .. } => ids[0],
            Self::Cut(_) | Self::Looks(_) | Self::Wrenched(_) | Self::Replaced { .. } => {
                unreachable!("undone as a whole")
            }
            Self::Plant(id)
            | Self::Color(id, _)
            | Self::ColorEffect(id, _)
            | Self::ShapeEffect(id, _)
            | Self::Print(id, _) => id,
        }
    }
}

impl UndoEntry {
    /// Follow bricks that came back under new ids, from the `at`th brick
    /// on as far as `budget` allows: the brick it got to (all of them when
    /// done).
    fn rename_from(&mut self, renamed: &Renamed, at: usize, budget: &mut u32) -> usize {
        fn walk<T>(
            items: &mut [T],
            renamed: &Renamed,
            at: usize,
            budget: &mut u32,
            id: impl Fn(&mut T) -> &mut BrickId,
        ) -> usize {
            let mut at = at;
            for item in items.iter_mut().skip(at) {
                if !spend(budget, work::SCAN) {
                    break;
                }
                let id = id(item);
                *id = renamed.get(*id).copied().unwrap_or(*id);
                at += 1;
            }
            at
        }
        match self {
            Self::Plant(id)
            | Self::Color(id, _)
            | Self::ColorEffect(id, _)
            | Self::ShapeEffect(id, _)
            | Self::Print(id, _) => walk(std::slice::from_mut(id), renamed, at, budget, |id| id),
            Self::Group { ids, .. } => walk(ids, renamed, at, budget, |id| id),
            Self::Cut(bricks) | Self::Wrenched(bricks) => {
                walk(bricks, renamed, at, budget, |(id, _)| id)
            }
            Self::Looks(looks) => walk(looks, renamed, at, budget, |(id, _)| id),
            Self::Replaced { removed, placed } => {
                let at = walk(removed, renamed, at, budget, |(id, _)| id);
                match at.checked_sub(removed.len()) {
                    Some(past) => removed.len() + walk(placed, renamed, past, budget, |id| id),
                    None => at,
                }
            }
        }
    }
}

/// Old brick ids and the new ids their bricks came back under.
pub(super) type Renamed = crate::id_map::IdMap<BrickId>;

/// Following bricks that came back under new ids through their owner's
/// undo steps and held copy, a slice at a time ([`Session::follow_some`]).
pub(super) struct Follow {
    renamed: Renamed,
    /// The step being followed through, by serial, and how far into it.
    step: u64,
    at: usize,
    /// The last step to follow through: later ones came after the bricks
    /// were back.
    last: u64,
    /// How far into the held copy's bricks.
    sources: usize,
}
impl Follow {
    pub fn new(s: &Session, owner: OwnerId, renamed: Renamed) -> Self {
        Self {
            renamed,
            step: 0,
            at: 0,
            last: s.undo.get(&owner).map_or(0, |stack| stack.serial),
            sources: 0,
        }
    }
}

impl UndoEntry {
    /// How many bricks undoing it changes.
    fn bricks(&self) -> usize {
        match self {
            Self::Group { ids, .. } => ids.len(),
            Self::Cut(bricks) | Self::Wrenched(bricks) => bricks.len(),
            Self::Looks(looks) => looks.len(),
            Self::Replaced { removed, placed } => removed.len() + placed.len(),
            Self::Plant(_)
            | Self::Color(..)
            | Self::ColorEffect(..)
            | Self::ShapeEffect(..)
            | Self::Print(..) => 1,
        }
    }
}

/// One step on the stack: what to undo, and the Add-On whose copy made it.
#[derive(Debug)]
pub(super) struct Step {
    entry: UndoEntry,
    by: Option<String>,
    /// Tells this step from any other, for an undo asked twice.
    serial: u64,
}

/// `QueueSO`: a ring that forgets its oldest entry when full.
#[derive(Debug, Default)]
pub(super) struct UndoStack {
    steps: VecDeque<Step>,
    serial: u64,
    /// The step the last undo held back to be asked again
    /// (`ndUndoConfirm`).
    asked: Option<u64>,
}
impl UndoStack {
    fn push(&mut self, entry: UndoEntry, by: Option<String>) {
        if self.steps.len() == UNDO_QUEUE_SIZE - 1
            && let Some(step) = self.steps.pop_front()
            && step.entry.bricks() > 1
        {
            copy_jobs::drop_later(step);
        }
        self.serial += 1;
        self.steps.push_back(Step {
            entry,
            by,
            serial: self.serial,
        });
    }
}

impl Session {
    pub(super) fn push_undo(&mut self, owner: OwnerId, entry: UndoEntry) {
        self.push_copy_undo(owner, entry, None);
    }

    /// A step `package`'s copy made: undoing it may be asked twice
    /// (`undo_confirm_over`).
    pub(super) fn push_copy_undo(&mut self, owner: OwnerId, entry: UndoEntry, by: Option<String>) {
        self.undo.entry(owner).or_default().push(entry, by);
    }

    /// Whether undoing `step` waits for a second Ctrl+Z: it is one of an
    /// Add-On's copy steps, bigger than that Add-On's `undo_confirm_over`,
    /// and not the step the last undo held back. The Add-On's `on_copy`
    /// hears it (`action` `"undo"`, `bricks`).
    fn hold_undo(&mut self, owner: OwnerId, step: &Step) -> bool {
        let Some(package) = &step.by else {
            return false;
        };
        let over = self.packages.as_ref().and_then(|host| {
            host.catalog
                .packages
                .get(package)
                .and_then(|p| p.behaviour.as_ref())
                .and_then(|b| b.undo_confirm_over)
        });
        let bricks = step.entry.bricks();
        if over.is_none_or(|over| bricks <= over as usize) {
            return false;
        }
        let stack = self.undo.get_mut(&owner).expect("popped from it");
        if stack.asked == Some(step.serial) {
            return false;
        }
        stack.asked = Some(step.serial);
        let outcome = copy_store::CopyOutcome {
            working: false,
            names: Vec::new(),
            action: "undo",
            name: None,
            bricks,
            total: bricks,
            placed: 0,
            limit_reached: false,
            refused: 0,
            error: None,
        };
        let package = package.clone();
        self.report_copy(&package, owner, outcome);
        true
    }

    /// `serverCmdUndoBrick`. Replies with the brick the popped entry
    /// changed, or `None` when nothing changed.
    pub(super) fn undo_brick(&mut self, owner: OwnerId) -> Result<Reply> {
        // One undo at a time, as the New Duplicator (`ndUndoInProgress`).
        if self.copy_working(owner) {
            self.center_print(owner, "Your duplicator is still working. Cancel it first.".into());
            return Ok(Reply::Undone(None));
        }
        let Some(step) = self
            .undo
            .get_mut(&owner)
            .and_then(|stack| stack.steps.pop_back())
        else {
            return Ok(Reply::Undone(None));
        };
        if self.hold_undo(owner, &step) {
            let stack = self.undo.get_mut(&owner).expect("popped from it");
            stack.steps.push_back(step);
            return Ok(Reply::Undone(None));
        }
        if let Some(stack) = self.undo.get_mut(&owner) {
            stack.asked = None;
        }
        let Step { entry, by, .. } = step;
        match entry {
            UndoEntry::Group { ids, group } => return self.undo_group(owner, ids, group, by),
            UndoEntry::Cut(bricks) => return self.undo_cut(owner, bricks, by),
            UndoEntry::Looks(looks) => {
                return self.undo_edits(owner, jobs::Edits::Looks(looks), by);
            }
            UndoEntry::Wrenched(bricks) => {
                return self.undo_edits(owner, jobs::Edits::Wrenched(bricks), by);
            }
            UndoEntry::Replaced { removed, placed } => {
                return self.undo_replaced(owner, removed, placed, by);
            }
            _ => {}
        }
        let id = entry.brick();
        let Some(brick_owner) = self.simulation.state().bricks.get(&id).map(|b| b.owner) else {
            return Ok(Reply::Undone(None));
        };
        let tick = self.simulation.state().tick;
        self.play_thread_three(tick, owner, "undo");
        let actor = self
            .peers
            .get(&owner)
            .context("Unknown connection")?
            .actor
            .clone();
        let edit = match entry {
            UndoEntry::Group { .. }
            | UndoEntry::Cut(_)
            | UndoEntry::Looks(_)
            | UndoEntry::Wrenched(_)
            | UndoEntry::Replaced { .. } => {
                unreachable!("undone above")
            }
            UndoEntry::Plant(_) => {
                // Only a brick still in the undoer's own brick group.
                if brick_owner != owner {
                    return Ok(Reply::Undone(None));
                }
                // `undoTrustCheck`: a brick whose loss would strand others
                // needs Full trust from every brick group it touches above
                // and below. It is group-to-group trust with no admin bypass.
                if self.simulation.will_cause_chain_kill(id)? {
                    for neighbor in self.simulation.connected_bricks(id)? {
                        let group = self.simulation.state().bricks[&neighbor].owner;
                        if actor.trust_level(group) < level::FULL {
                            let name = self.brick_group_name(group);
                            self.center_print(
                                owner,
                                format!("{name} does not trust you enough to do that."),
                            );
                            return Ok(Reply::Undone(None));
                        }
                    }
                }
                self.tool_kill_brick(owner, id)?;
                return Ok(Reply::Undone(Some(id)));
            }
            UndoEntry::Color(_, color) => Edit::Color(color),
            UndoEntry::ColorEffect(_, effect) => Edit::ColorEffect(effect),
            UndoEntry::ShapeEffect(_, effect) => Edit::ShapeEffect(effect),
            UndoEntry::Print(_, print) => Edit::Print(print),
        };
        // `$TrustLevel::UndoPaint`, `UndoFXPaint` and `UndoPrint` are all Full.
        if owner == 0 || !actor.trusted(brick_owner, level::FULL) {
            // v20 names an unset `%brickGroup` here, so the group is blank.
            self.center_print(owner, " does not trust you enough to do that.".into());
            return Ok(Reply::Undone(None));
        }
        self.simulation.edit(&actor, id, edit)?;
        self.dirty.insert(id);
        Ok(Reply::Undone(Some(id)))
    }
}

impl Session {
    /// Undo a placed copy: each of its bricks still in the group it was
    /// planted into goes, last placed first. A brick joined to bricks
    /// outside the copy breaks as one undone plant does (`killBrick`, its
    /// chain kill and `undoTrustCheck`); the rest simply break, since the
    /// copy goes too. A big copy goes over several ticks.
    fn undo_group(
        &mut self,
        owner: OwnerId,
        ids: Vec<BrickId>,
        group: OwnerId,
        by: Option<String>,
    ) -> Result<Reply> {
        let tick = self.simulation.state().tick;
        self.play_thread_three(tick, owner, "undo");
        let work = jobs::UndoGroup::new(self, owner, ids, group, by.clone())?;
        Ok(match self.begin_copy_job(owner, by, work)? {
            Some(work) => work.complete(),
            None => Reply::Undone(None),
        })
    }

    /// Undo a cut: every brick goes back as it was, or none does while
    /// something stands in the way, and the step stays to try again.
    fn undo_cut(
        &mut self,
        owner: OwnerId,
        bricks: Vec<(BrickId, Brick)>,
        by: Option<String>,
    ) -> Result<Reply> {
        let tick = self.simulation.state().tick;
        self.play_thread_three(tick, owner, "undo");
        let work = jobs::UndoCut::new(bricks, by.clone());
        Ok(match self.begin_copy_job(owner, by, work)? {
            Some(work) => work.complete(self, owner),
            None => Reply::Undone(None),
        })
    }

    /// Undo painting or a fill wrench: each brick still standing that the
    /// undoer may change gets its old paint or settings back.
    fn undo_edits(&mut self, owner: OwnerId, edits: jobs::Edits, by: Option<String>) -> Result<Reply> {
        let tick = self.simulation.state().tick;
        self.play_thread_three(tick, owner, "undo");
        let work = jobs::UndoEdits::new(self, owner, edits, by.clone())?;
        Ok(match self.begin_copy_job(owner, by, work)? {
            Some(work) => work.complete(),
            None => Reply::Undone(None),
        })
    }
}

impl Session {
    /// Bricks that came back under new ids: `owner`'s earlier steps and
    /// copy name them by those now.
    /// Follow bricks that came back under new ids through `owner`'s undo
    /// steps and held copy, as far as `budget` allows: true once done.
    pub(super) fn follow_some(&mut self, owner: OwnerId, f: &mut Follow, budget: &mut u32) -> bool {
        if let Some(stack) = self.undo.get_mut(&owner) {
            while f.step <= f.last {
                // Steps forgotten meanwhile are skipped.
                let i = stack.steps.partition_point(|step| step.serial < f.step);
                let Some(step) = stack.steps.get_mut(i).filter(|step| step.serial <= f.last) else {
                    f.step = f.last + 1;
                    break;
                };
                if step.serial != f.step {
                    (f.step, f.at) = (step.serial, 0);
                }
                f.at = step.entry.rename_from(&f.renamed, f.at, budget);
                if f.at < step.entry.bricks() {
                    return false;
                }
                (f.step, f.at) = (f.step + 1, 0);
            }
        }
        if let Some(copy) = self.copies.get_mut(&owner) {
            let ids = std::sync::Arc::make_mut(&mut copy.sources);
            for id in ids.iter_mut().skip(f.sources) {
                if !spend(budget, work::SCAN) {
                    return false;
                }
                *id = f.renamed.get(*id).copied().unwrap_or(*id);
                f.sources += 1;
            }
        }
        true
    }

    /// [`Self::follow_some`] to the end now, whatever it costs.
    pub(super) fn follow_all(&mut self, owner: OwnerId, follow: &mut Follow) {
        loop {
            let mut budget = u32::MAX;
            if self.follow_some(owner, follow, &mut budget) {
                break;
            }
        }
    }

    /// Follow bricks that came back under new ids, all at once.
    pub(super) fn follow_renamed(&mut self, owner: OwnerId, pairs: Vec<(BrickId, BrickId)>) {
        let mut renamed = Renamed::default();
        for (old, new) in pairs {
            renamed.insert(old, new);
        }
        let mut follow = Follow::new(self, owner, renamed);
        self.follow_all(owner, &mut follow);
    }
}
