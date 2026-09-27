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
            Self::Plant(id)
            | Self::Color(id, _)
            | Self::ColorEffect(id, _)
            | Self::ShapeEffect(id, _)
            | Self::Print(id, _) => id,
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
