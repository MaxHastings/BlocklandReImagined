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
    /// A placed copy (`place_blueprint`): one Ctrl+Z takes it all back.
    Group(Vec<BrickId>),
    /// Bricks cut away (`cut_copy`), with the ids they had, as they were:
    /// one Ctrl+Z puts them all back.
    Cut(Vec<(BrickId, Brick)>),
    /// Bricks painted together (`paint_copy`), with the colour each had.
    Colors(Vec<(BrickId, u8)>),
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
            Self::Group(ref ids) => ids[0],
            Self::Cut(_) | Self::Colors(_) => unreachable!("undone as a whole"),
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
            Self::Group(ids) => ids.iter_mut().for_each(follow),
            Self::Cut(bricks) => bricks.iter_mut().for_each(|(id, _)| follow(id)),
            Self::Colors(colors) => colors.iter_mut().for_each(|(id, _)| follow(id)),
        }
    }
}

/// `QueueSO`: a ring that forgets its oldest entry when full.
#[derive(Debug, Default)]
pub(super) struct UndoStack(VecDeque<UndoEntry>);
impl UndoStack {
    fn push(&mut self, entry: UndoEntry) {
        if self.0.len() == UNDO_QUEUE_SIZE - 1 {
            self.0.pop_front();
        }
        self.0.push_back(entry);
    }
}

impl Session {
    pub(super) fn push_undo(&mut self, owner: OwnerId, entry: UndoEntry) {
        self.undo.entry(owner).or_default().push(entry);
    }

    /// `serverCmdUndoBrick`. Replies with the brick the popped entry
    /// changed, or `None` when nothing changed.
    pub(super) fn undo_brick(&mut self, owner: OwnerId) -> Result<Reply> {
        let Some(entry) = self
            .undo
            .get_mut(&owner)
            .and_then(|stack| stack.0.pop_back())
        else {
            return Ok(Reply::Undone(None));
        };
        match entry {
            UndoEntry::Group(ids) => return self.undo_group(owner, ids),
            UndoEntry::Cut(bricks) => return self.undo_cut(owner, bricks),
            UndoEntry::Colors(colors) => return self.undo_colors(owner, colors),
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
            UndoEntry::Group(_) | UndoEntry::Cut(_) | UndoEntry::Colors(_) => {
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
    /// Undo a placed copy: each of its bricks still in the undoer's group
    /// goes, last placed first. A brick joined to bricks outside the copy
    /// breaks as one undone plant does (`killBrick`, its chain kill and
    /// `undoTrustCheck`); the rest simply break, since the copy goes too.
    fn undo_group(&mut self, owner: OwnerId, ids: Vec<BrickId>) -> Result<Reply> {
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
            if brick.owner != owner {
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
    fn undo_cut(&mut self, owner: OwnerId, bricks: Vec<(BrickId, Brick)>) -> Result<Reply> {
        let tick = self.simulation.state().tick;
        self.play_thread_three(tick, owner, "undo");
        let restored = bricks.iter().map(|(_, b)| b.clone()).collect();
        match self.simulation.restore_group(restored) {
            Ok(ids) => {
                self.dirty.extend(ids.iter().copied());
                // The bricks came back under new ids: this player's earlier
                // steps and copy name them by those now.
                let renamed: BTreeMap<BrickId, BrickId> = bricks
                    .iter()
                    .map(|(old, _)| *old)
                    .zip(ids.iter().copied())
                    .collect();
                if let Some(stack) = self.undo.get_mut(&owner) {
                    for entry in &mut stack.0 {
                        entry.rename(&renamed);
                    }
                }
                if let Some(sources) = self.copy_sources.get_mut(&owner) {
                    for id in sources {
                        *id = renamed.get(id).copied().unwrap_or(*id);
                    }
                }
                Ok(Reply::Undone(ids.first().copied()))
            }
            Err(error) => {
                let text = match error.downcast_ref::<crate::simulation::PlantFailure>() {
                    Some(_) => "Something is in the way of the bricks you cut.".to_string(),
                    None => format!("{error:#}"),
                };
                self.center_print(owner, text);
                self.push_undo(owner, UndoEntry::Cut(bricks));
                Ok(Reply::Undone(None))
            }
        }
    }

    /// Undo painting a copy's bricks: each still standing that the undoer
    /// may paint takes its old colour back.
    fn undo_colors(&mut self, owner: OwnerId, colors: Vec<(BrickId, u8)>) -> Result<Reply> {
        let tick = self.simulation.state().tick;
        self.play_thread_three(tick, owner, "undo");
        let actor = self
            .peers
            .get(&owner)
            .context("Unknown connection")?
            .actor
            .clone();
        let palette = self.simulation.state().palette.len();
        let mut first = None;
        for (id, color) in colors {
            let Some(brick) = self.simulation.state().bricks.get(&id) else {
                continue;
            };
            if !actor.trusted(brick.owner, level::FULL) || usize::from(color) >= palette {
                continue;
            }
            self.simulation.mutate(id, |b| b.color = color)?;
            self.dirty.insert(id);
            first.get_or_insert(id);
        }
        Ok(Reply::Undone(first))
    }
}
