//! v20's per-client undo stack (`%client.undoStack`) and `serverCmdUndoBrick`.
//!
//! One mixed stack records plants, spray paint, FX paint and prints. Each
//! Ctrl+Z pops exactly one entry: an entry whose brick is gone is spent
//! without effect, as in v20. Undoing a plant is a `killBrick`, so the brick
//! breaks with the hammer's sound and debris.
use super::*;
use bri_world::authority::trust as level;

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
    /// Follow bricks that came back under new ids.
    fn rename(&mut self, renamed: &BTreeMap<BrickId, BrickId>) {
        let follow = |id: &mut BrickId| *id = renamed.get(id).copied().unwrap_or(*id);
        match self {
            Self::Plant(id)
            | Self::Color(id, _)
            | Self::ColorEffect(id, _)
            | Self::ShapeEffect(id, _)
            | Self::Print(id, _) => follow(id),
            Self::Group { ids, .. } => ids.iter_mut().for_each(follow),
            Self::Cut(bricks) => bricks.iter_mut().for_each(|(id, _)| follow(id)),
            Self::Looks(looks) => looks.iter_mut().for_each(|(id, _)| follow(id)),
            Self::Wrenched(bricks) => bricks.iter_mut().for_each(|(id, _)| follow(id)),
            Self::Replaced { removed, placed } => {
                removed.iter_mut().for_each(|(id, _)| follow(id));
                placed.iter_mut().for_each(follow);
            }
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
        if self.steps.len() == UNDO_QUEUE_SIZE - 1 {
            self.steps.pop_front();
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
            UndoEntry::Group { ids, group } => return self.undo_group(owner, ids, group),
            UndoEntry::Cut(bricks) => return self.undo_cut(owner, bricks, by),
            UndoEntry::Looks(looks) => return self.undo_looks(owner, looks),
            UndoEntry::Wrenched(bricks) => return self.undo_wrenched(owner, bricks),
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
    /// copy goes too.
    fn undo_group(&mut self, owner: OwnerId, ids: Vec<BrickId>, group: OwnerId) -> Result<Reply> {
        let tick = self.simulation.state().tick;
        self.play_thread_three(tick, owner, "undo");
        let actor = self
            .peers
            .get(&owner)
            .context("Unknown connection")?
            .actor
            .clone();
        let copy: BTreeSet<BrickId> = ids.iter().copied().collect();
        let mut first = None;
        for &id in ids.iter().rev() {
            let Some(brick) = self.simulation.state().bricks.get(&id) else {
                continue;
            };
            if brick.owner != group {
                continue;
            }
            let outside: Vec<OwnerId> = self
                .simulation
                .connected_bricks(id)?
                .into_iter()
                .filter(|n| !copy.contains(n))
                .map(|n| self.simulation.state().bricks[&n].owner)
                .collect();
            if outside.is_empty() {
                self.kill_one_brick(&actor, id, None)?;
                self.close_inspections(id);
            } else {
                let untrusting = outside
                    .into_iter()
                    .find(|&group| actor.trust_level(group) < level::FULL);
                if let Some(group) = untrusting
                    && self.simulation.will_cause_chain_kill(id)?
                {
                    let name = self.brick_group_name(group);
                    self.center_print(
                        owner,
                        format!("{name} does not trust you enough to do that."),
                    );
                    continue;
                }
                self.tool_kill_brick(owner, id)?;
            }
            first.get_or_insert(id);
        }
        Ok(Reply::Undone(first))
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
        let restored = bricks.iter().map(|(_, b)| b.clone()).collect();
        match self.simulation.restore_group(restored) {
            Ok(ids) => {
                self.dirty.extend(ids.iter().copied());
                let renamed: BTreeMap<BrickId, BrickId> = bricks
                    .iter()
                    .map(|(old, _)| *old)
                    .zip(ids.iter().copied())
                    .collect();
                self.follow_renamed(owner, &renamed);
                Ok(Reply::Undone(ids.first().copied()))
            }
            Err(error) => {
                let text = match error.downcast_ref::<crate::simulation::PlantFailure>() {
                    Some(_) => "Something is in the way of the bricks you cut.".to_string(),
                    None => format!("{error:#}"),
                };
                self.center_print(owner, text);
                self.push_copy_undo(owner, UndoEntry::Cut(bricks), by);
                Ok(Reply::Undone(None))
            }
        }
    }
}

impl Session {
    /// Bricks that came back under new ids: `owner`'s earlier steps and
    /// copy name them by those now.
    pub(super) fn follow_renamed(&mut self, owner: OwnerId, renamed: &BTreeMap<BrickId, BrickId>) {
        if let Some(stack) = self.undo.get_mut(&owner) {
            for step in &mut stack.steps {
                step.entry.rename(renamed);
            }
        }
        if let Some(copy) = self.copies.get_mut(&owner) {
            for id in &mut copy.sources {
                *id = renamed.get(id).copied().unwrap_or(*id);
            }
        }
    }
}
