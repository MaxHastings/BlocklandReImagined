//! Copies of builds held for players, and placing them (see
//! [`crate::blueprint`]). An Add-On decides what to copy and for whom
//! (`copy_build`, `copy_box`); placing is the player's own command and
//! follows the plant rules: minigame building, reach, trust, overlap,
//! support, the brick limit, and the plant rate, which a copy uses up as
//! one plant window of its own.
//!
//! A copy remembers the bricks it was taken from, so an Add-On may cut
//! them away (`cut_copy`) or change them (see `copy_edits`) with the
//! player's own trust, each as one step of their undo. A copy may be held
//! as a selection first (`hidden`), shown to place when the Add-On says.
use super::*;
use crate::blueprint::{Blueprint, MAX_BLUEPRINT_BRICKS, Outline, snap_anchor};
use bri_package_runtime::ops::{CopyHold, CopyRule, CopyTrust, MirrorAxis, StackReach};
use bri_world::authority::trust as level;
use super::copy_store::CopyOutcome;

/// A copy a player holds: the bricks it was taken from (`cut_copy`,
/// `paint_copy`, `highlight_copy`), the Add-On that took it and how it
/// plants.
pub(super) struct HeldCopy {
    pub sources: Vec<BrickId>,
    pub package: String,
    pub partial: bool,
    /// The player has it to place; else it is a selection only.
    pub shown: bool,
    /// Every plant of it may float ([`Session::float_copy`]).
    pub float: bool,
    /// The next plant may float, until this tick ([`Session::plant_copy`]).
    pub float_once: Option<u64>,
    /// The fill wrench is open on its bricks ([`Session::open_copy_wrench`]).
    pub wrench_open: bool,
    /// The brick group its plants go into instead of the player's own
    /// ([`Session::plant_as`]).
    pub plant_as: Option<PlantAs>,
}

/// Another player's brick group a copy is planted into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PlantAs {
    pub group: OwnerId,
    /// The group as players know it: its owner's name, or BL_ID.
    pub name: String,
    /// Administrators plant into it without its trust.
    pub admin: bool,
}

/// Why a copy's plant was refused before any brick was tried.
#[derive(Debug, Clone, PartialEq)]
pub enum CopyRefusal {
    /// The player's plant wait ([`Session::plant_wait`]) has this many
    /// seconds left.
    Wait(f32),
    /// The player no longer has build trust with the group they plant
    /// into, named.
    Group(String),
}
impl std::fmt::Display for CopyRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wait(seconds) => {
                let whole = seconds.ceil().max(1.0) as u32;
                let s = if whole == 1 { "" } else { "s" };
                write!(f, "You need to wait {whole} second{s} before planting again!")
            }
            Self::Group(name) => write!(
                f,
                "You need build trust with {name} to plant bricks in their group."
            ),
        }
    }
}
impl std::error::Error for CopyRefusal {}

/// The pause after each copy plant ([`Session::plant_wait`]), in ticks,
/// and the first tick the next may come.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct PlantWait {
    pub ticks: u64,
    pub next: u64,
}
impl HeldCopy {
    pub fn new(sources: Vec<BrickId>, package: &str, partial: bool) -> Self {
        Self {
            sources,
            package: package.into(),
            partial,
            shown: true,
            float: false,
            float_once: None,
            wrench_open: false,
            plant_as: None,
        }
    }
}

/// What a copy took, or why it took nothing: handed to the Add-On's
/// `on_copy` hook, else told to the player.
pub struct Copied {
    pub selection: crate::simulation::Selection,
    /// `trust`, `public`, `empty` or `invalid`.
    pub error: Option<(&'static str, String)>,
}

/// Whether `rule` lets `actor` copy `brick`.
fn admits(actor: &Actor, rule: CopyRule, brick: &Brick) -> bool {
    if brick.owner == 0 {
        return rule.public;
    }
    if actor.administrator && rule.admin {
        return true;
    }
    let needed = match rule.trust {
        CopyTrust::Build => level::BUILD,
        CopyTrust::Full => level::FULL,
    };
    actor.trust_level(brick.owner) >= needed
}

impl Session {
    /// Copy the stack at `brick` for `owner` as `rule` allows
    /// ([`crate::simulation::Simulation::select_stack`]), replacing any
    /// copy they hold, and send it to them to place with `tool`.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_build(
        &mut self,
        owner: OwnerId,
        brick: BrickId,
        limit: usize,
        reach: StackReach,
        rule: CopyRule,
        tool: &str,
        package: &str,
    ) -> Copied {
        self.copy_build_held(
            owner,
            brick,
            limit,
            reach,
            rule,
            tool,
            package,
            CopyHold::default(),
        )
    }

    /// [`Self::copy_build`], held as `hold` says: as a selection to show
    /// later, or added to the copy the player holds from `package`.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_build_held(
        &mut self,
        owner: OwnerId,
        brick: BrickId,
        limit: usize,
        reach: StackReach,
        rule: CopyRule,
        tool: &str,
        package: &str,
        hold: CopyHold,
    ) -> Copied {
        let selected = (|| {
            self.check_copy(limit, tool)?;
            let actor = &self.peers.get(&owner).context("Unknown connection")?.actor;
            let first = self
                .simulation
                .state()
                .bricks
                .get(&brick)
                .context("Unknown brick")?;
            if !admits(actor, rule, first) {
                return Ok(Err(if first.owner == 0 {
                    ("public", "Public bricks cannot be copied.".to_string())
                } else {
                    (
                        "trust",
                        "The brick's owner does not trust you enough to do that.".to_string(),
                    )
                }));
            }
            let reach = crate::simulation::StackReach {
                up: reach.up,
                limited: reach.limited,
            };
            Ok(Ok(self.simulation.select_stack(
                brick,
                reach,
                limit,
                |b| admits(actor, rule, b),
            )?))
        })();
        self.finish_copy(owner, selected, rule, tool, package, limit, hold)
    }

    /// Copy every brick wholly inside the box from `min` to `max` (world
    /// units, grown out to the grid; not `limited`, every brick reaching
    /// into it) that `rule` lets `owner` take, as [`Self::copy_build`]
    /// does.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_box(
        &mut self,
        owner: OwnerId,
        min: [f32; 3],
        max: [f32; 3],
        limited: bool,
        limit: usize,
        rule: CopyRule,
        tool: &str,
        package: &str,
    ) -> Copied {
        self.copy_box_held(
            owner,
            (min, max),
            limited,
            limit,
            rule,
            tool,
            package,
            CopyHold::default(),
        )
    }

    /// [`Self::copy_box`], held as `hold` says.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_box_held(
        &mut self,
        owner: OwnerId,
        (min, max): ([f32; 3], [f32; 3]),
        limited: bool,
        limit: usize,
        rule: CopyRule,
        tool: &str,
        package: &str,
        hold: CopyHold,
    ) -> Copied {
        let selected = (|| {
            self.check_copy(limit, tool)?;
            let area = grid_box(min, max)?;
            let actor = &self.peers.get(&owner).context("Unknown connection")?.actor;
            let selection = self
                .simulation
                .select_box(area, limited, limit, |b| admits(actor, rule, b));
            Ok(if selection.bricks.is_empty() && selection.refused > 0 {
                Err((
                    "trust",
                    "The bricks in that box belong to builds that do not trust you enough."
                        .to_string(),
                ))
            } else if selection.bricks.is_empty() {
                Err((
                    "empty",
                    if limited {
                        "There are no bricks wholly inside that box."
                    } else {
                        "There are no bricks in that box."
                    }
                    .to_string(),
                ))
            } else {
                Ok(selection)
            })
        })();
        self.finish_copy(owner, selected, rule, tool, package, limit, hold)
    }

    fn check_copy(&self, limit: usize, tool: &str) -> Result<()> {
        ensure!(
            (1..=MAX_BLUEPRINT_BRICKS).contains(&limit),
            "A copy holds 1 to {MAX_BLUEPRINT_BRICKS} bricks"
        );
        ensure!(
            self.weapons.contains_item(tool),
            "The copy's tool {tool} is not an item on this server"
        );
        Ok(())
    }

    /// Give `owner` the copy a selection made, or say why there is none.
    #[allow(clippy::too_many_arguments)]
    fn finish_copy(
        &mut self,
        owner: OwnerId,
        selected: Result<std::result::Result<crate::simulation::Selection, (&'static str, String)>>,
        rule: CopyRule,
        tool: &str,
        package: &str,
        limit: usize,
        hold: CopyHold,
    ) -> Copied {
        let failed = |error| Copied {
            selection: Default::default(),
            error: Some(error),
        };
        let mut selection = match selected {
            Ok(Ok(selection)) => selection,
            Ok(Err(error)) => return failed(error),
            Err(error) => return failed(("invalid", format!("{error:#}"))),
        };
        if hold.add {
            self.add_to_held(owner, package, &mut selection, limit);
        }
        let mut held = HeldCopy::new(selection.bricks.clone(), package, rule.partial);
        held.shown = !hold.hidden;
        match self.hold_copy(owner, held, tool) {
            Ok(()) => Copied {
                selection,
                error: None,
            },
            Err(error) => failed(("invalid", format!("{error:#}"))),
        }
    }

    /// Put the bricks of the copy `owner` holds from `package` that still
    /// stand ahead of `selection`'s, each once, at most `limit` in all.
    fn add_to_held(
        &self,
        owner: OwnerId,
        package: &str,
        selection: &mut crate::simulation::Selection,
        limit: usize,
    ) {
        let Some(held) = self.copies.get(&owner).filter(|c| c.package == package) else {
            return;
        };
        let world = self.simulation.state();
        let mut taken: BTreeSet<BrickId> = BTreeSet::new();
        let mut bricks = Vec::with_capacity(held.sources.len() + selection.bricks.len());
        for &id in held.sources.iter().chain(&selection.bricks) {
            if world.bricks.contains_key(&id) && taken.insert(id) {
                bricks.push(id);
            }
        }
        if bricks.len() > limit {
            bricks.truncate(limit);
            selection.limit_reached = true;
        }
        selection.bricks = bricks;
    }

    /// Give `owner` a copy of `held.sources` to place with `tool`, each
    /// brick as it is under any highlight.
    fn hold_copy(&mut self, owner: OwnerId, held: HeldCopy, tool: &str) -> Result<()> {
        let world = self.simulation.state();
        let bricks: Vec<Brick> = held
            .sources
            .iter()
            .map(|id| self.unlit(*id, &world.bricks[id]))
            .collect();
        let blueprint = Blueprint::capture(tool, &bricks, &self.simulation.definitions)?;
        self.hold_blueprint(owner, blueprint, held);
        Ok(())
    }

    /// Give `owner` `blueprint` to place (or, not `held.shown`, to hold as
    /// a selection), replacing any copy they hold.
    pub(super) fn hold_blueprint(&mut self, owner: OwnerId, blueprint: Blueprint, held: HeldCopy) {
        let was_shown = self.copies.get(&owner).is_some_and(|c| c.shown);
        if held.shown {
            self.notify(owner, Notice::Blueprint(Some(Box::new(blueprint.clone()))));
        } else if was_shown {
            self.notify(owner, Notice::Blueprint(None));
        }
        self.blueprints.insert(owner, blueprint);
        self.copies.insert(owner, held);
    }

    /// Give `owner` the copy they hold as a selection to place, where it
    /// was taken. Nothing changes when they have it already.
    pub fn show_copy(&mut self, owner: OwnerId) -> Result<()> {
        let held = self.copies.get_mut(&owner).context("Copy a build first")?;
        if !held.shown {
            held.shown = true;
            // Taken up to place, the selection stops glowing.
            let sources = held.sources.clone();
            self.unlight_bricks(&sources)?;
            let blueprint = self.blueprints[&owner].clone();
            self.notify(owner, Notice::Blueprint(Some(Box::new(blueprint))));
        }
        Ok(())
    }

    /// Keep the copy `owner` holds as a selection only.
    pub fn hide_copy(&mut self, owner: OwnerId) {
        if let Some(held) = self.copies.get_mut(&owner)
            && held.shown
        {
            held.shown = false;
            self.notify(owner, Notice::Blueprint(None));
        }
    }

    /// The copy `owner` holds and places, or why there is none.
    fn shown_copy(&mut self, owner: OwnerId) -> Result<&mut HeldCopy> {
        let held = self.copies.get_mut(&owner).context("Copy a build first")?;
        ensure!(held.shown, "Show the copy before moving it");
        Ok(held)
    }

    /// Move the copy `owner` places as their brick shift keys would.
    pub fn shift_copy(&mut self, owner: OwnerId, offset: [i32; 3], super_shift: bool) -> Result<()> {
        self.shown_copy(owner)?;
        self.notify(owner, Notice::ShiftCopy { offset, super_shift });
        Ok(())
    }

    /// Turn the copy `owner` places as their rotate keys would.
    pub fn rotate_copy(&mut self, owner: OwnerId, direction: i8) -> Result<()> {
        self.shown_copy(owner)?;
        self.notify(owner, Notice::RotateCopy { direction });
        Ok(())
    }

    /// Ask `owner`'s game to plant the copy where it stands. With `float`,
    /// that plant (if it comes within a second) may float.
    pub fn plant_copy(&mut self, owner: OwnerId, float: bool) -> Result<()> {
        let until = self.simulation.state().tick + bri_world::TICKS_PER_SECOND;
        let held = self.shown_copy(owner)?;
        if float {
            held.float_once = Some(until);
        }
        self.notify(owner, Notice::PlantCopy);
        Ok(())
    }

    /// Let every plant of the copy `owner` holds float, or not.
    pub fn float_copy(&mut self, owner: OwnerId, float: bool) -> Result<()> {
        self.copies
            .get_mut(&owner)
            .context("Copy a build first")?
            .float = float;
        Ok(())
    }

    /// Light the bricks `owner`'s copy was taken from in the palette
    /// colour nearest `rgba` (or their own), glowing, for `seconds`.
    pub fn highlight_copy(
        &mut self,
        owner: OwnerId,
        rgba: Option<[f32; 4]>,
        seconds: f32,
    ) -> Result<()> {
        let ids = self.copy_originals(owner)?;
        let color = rgba.map(|rgba| self.closest_paint(rgba));
        self.light_bricks(&ids, color, super::highlight::GLOW, seconds)
    }

    /// Mirror the copy `owner` holds, as they see and plant it. The copy
    /// itself does not change: the mirror is part of where they put it,
    /// like its turn, and travels with `PlaceBlueprint`.
    pub fn mirror_copy(&mut self, owner: OwnerId, axis: MirrorAxis) -> Result<()> {
        ensure!(
            self.copies.get(&owner).is_some_and(|c| c.shown),
            "Copy a build before mirroring it"
        );
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        let across_z = match axis {
            MirrorAxis::Y => {
                self.notify(owner, Notice::FlipCopy);
                return Ok(());
            }
            MirrorAxis::X => false,
            MirrorAxis::Z => true,
            // Facing north or south, left and right lie along x.
            MirrorAxis::View => {
                let facing = crate::ghost::cardinal(peer.player.state().forward());
                facing.x.abs() > facing.z.abs()
            }
        };
        self.notify(owner, Notice::MirrorCopy { across_z });
        Ok(())
    }

    /// Move the copy `owner` holds against the surface at `point` facing
    /// out along `normal`. Where the copy stands is the player's to choose,
    /// like its turn, so the host only tells them.
    pub fn move_copy(&mut self, owner: OwnerId, point: [f32; 3], normal: [f32; 3]) -> Result<()> {
        self.shown_copy(owner)?;
        self.notify(owner, Notice::MoveCopy { point, normal });
        Ok(())
    }

    /// Take away the copy `owner` holds, if any.
    pub fn drop_copy(&mut self, owner: OwnerId) {
        if self.blueprints.contains_key(&owner) {
            self.forget_blueprint(owner);
            self.notify(owner, Notice::Blueprint(None));
        }
    }

    /// Make `owner` wait `seconds` after each copy plant before the next.
    pub fn plant_wait(&mut self, owner: OwnerId, seconds: f32) -> Result<()> {
        ensure!(
            seconds.is_finite() && (0.0..=60.0).contains(&seconds),
            "A plant wait is 0 to 60 seconds"
        );
        ensure!(self.peers.contains_key(&owner), "No such player");
        let ticks = (seconds * bri_world::TICKS_PER_SECOND as f32).round() as u64;
        let wait = self.plant_waits.entry(owner).or_default();
        // A shorter wait shortens the one running now.
        wait.next = wait.next.saturating_sub(wait.ticks.saturating_sub(ticks));
        wait.ticks = ticks;
        Ok(())
    }

    /// Show `owner` their copy turning about the whole of it (`whole`),
    /// else about the brick it was taken from first, and so put against
    /// what they click.
    pub fn pivot_copy(&mut self, owner: OwnerId, whole: bool) -> Result<()> {
        ensure!(self.peers.contains_key(&owner), "No such player");
        self.notify(owner, Notice::PivotCopy { whole });
        Ok(())
    }

    /// The brick group `target` names: an online player's by their name
    /// (whole, else part of it, in any case) or BL_ID, else the group
    /// of bricks a BL_ID owns. With the group's name.
    fn find_group(&self, target: &str) -> Option<(OwnerId, String)> {
        let lower = target.to_lowercase();
        let named = |exact: bool| {
            self.peers.iter().find(|(_, p)| {
                let name = p.name.to_lowercase();
                if exact { name == lower } else { name.contains(&lower) }
            })
        };
        if let Some((id, peer)) = named(true).or_else(|| named(false)) {
            return Some((*id, peer.name.clone()));
        }
        let id: OwnerId = target.parse().ok().filter(|id| *id != 0)?;
        if let Some(peer) = self.peers.get(&id) {
            return Some((id, peer.name.clone()));
        }
        self.simulation
            .state()
            .bricks
            .values()
            .any(|b| b.owner == id)
            .then(|| (id, format!("BL_ID: {id}")))
    }

    /// Whether `owner` may plant into `group`: with build trust, or as an
    /// administrator when `admin`.
    fn may_plant_into(&self, owner: OwnerId, group: OwnerId, admin: bool) -> bool {
        self.peers.get(&owner).is_some_and(|p| {
            p.actor.trust_level(group) >= level::BUILD || (admin && p.actor.administrator)
        })
    }

    /// Plant `owner`'s copies into the brick group `target` names, as
    /// [`bri_package_runtime::ops::Op::PlantAs`] says; empty plants into
    /// their own again. The group goes with the copy they hold.
    pub(super) fn plant_as(&mut self, owner: OwnerId, target: &str, admin: bool) -> CopyOutcome {
        let target = target.trim();
        if !self.copies.contains_key(&owner) {
            return CopyOutcome::failed("plant_as", false, anyhow::anyhow!("no copy"));
        }
        let chosen = if target.is_empty() {
            Ok(None)
        } else {
            match self.find_group(target) {
                None => Err((
                    Some(target.to_string()),
                    ("missing", format!("No brick group was found for \"{target}\".")),
                )),
                Some((group, name)) if !self.may_plant_into(owner, group, admin) => Err((
                    Some(name.clone()),
                    ("trust", CopyRefusal::Group(name).to_string()),
                )),
                Some((group, name)) => Ok(Some(PlantAs { group, name, admin })),
            }
        };
        let held = self.copies.get_mut(&owner).expect("checked");
        match chosen {
            Ok(plant_as) => {
                let name = plant_as.as_ref().map(|p| p.name.clone());
                held.plant_as = plant_as;
                CopyOutcome::about("plant_as", name, None)
            }
            Err((name, error)) => {
                held.plant_as = None;
                CopyOutcome::about("plant_as", name, Some(error))
            }
        }
    }

    /// The bricks `owner`'s copy was taken from that still stand.
    pub(super) fn copy_originals(&self, owner: OwnerId) -> Result<Vec<BrickId>> {
        let sources = &self
            .copies
            .get(&owner)
            .context("Copy a build first")?
            .sources;
        let world = self.simulation.state();
        let standing: Vec<BrickId> = sources
            .iter()
            .copied()
            .filter(|id| world.bricks.contains_key(id))
            .collect();
        ensure!(
            !standing.is_empty(),
            "The bricks this copy was taken from are gone"
        );
        Ok(standing)
    }

    /// Every brick in `ids` is one `owner` may change with full trust (the
    /// hammer's and the spray can's), else how many are not.
    pub(super) fn ensure_full_trust(&self, owner: OwnerId, ids: &[BrickId]) -> Result<()> {
        let actor = &self.peers.get(&owner).context("Unknown connection")?.actor;
        let world = self.simulation.state();
        let refused = ids
            .iter()
            .filter(|&&id| !actor.trusted(world.bricks[&id].owner, level::FULL))
            .count();
        ensure!(
            refused == 0,
            "{refused} of these bricks belong to builds that do not trust you enough."
        );
        Ok(())
    }

    /// Remove the bricks `owner`'s copy was taken from, all or none, as
    /// one undo step that puts them back exactly as they were. The copy
    /// stays in hand, so planting it elsewhere moves the build.
    pub fn cut_copy(&mut self, owner: OwnerId) -> Result<usize> {
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        combat::ensure_may_build(
            &peer.combat,
            &self.minigames,
            bri_minigames::BuildAction::Build,
        )?;
        let ids = self.copy_originals(owner)?;
        self.ensure_full_trust(owner, &ids)?;
        let world = self.simulation.state();
        let removed: Vec<(BrickId, Brick)> = ids
            .iter()
            .map(|id| (*id, self.unlit(*id, &world.bricks[id])))
            .collect();
        let middle = removed
            .iter()
            .fold(Vec3::ZERO, |sum, (_, b)| sum + Vec3::from(b.position))
            / removed.len() as f32;
        // Checked above; the engine removes them in one pass.
        let engine = Actor {
            administrator: true,
            ..Default::default()
        };
        self.simulation.remove_many(&engine, &ids)?;
        for &id in &ids {
            self.dirty.insert(id);
            self.events.respawns.remove(&id);
            self.close_inspections(id);
        }
        let by = self.copies.get(&owner).map(|c| c.package.clone());
        self.push_copy_undo(owner, undo::UndoEntry::Cut(removed), by);
        let tick = self.simulation.state().tick;
        self.cues
            .emit(tick, crate::presentation::CueKind::Plant, middle.to_array());
        Ok(ids.len())
    }

    /// Outline a box for `owner` while `tool` is in their hand, or take
    /// it away.
    pub fn show_box(
        &mut self,
        owner: OwnerId,
        area: Option<([f32; 3], [f32; 3])>,
        tool: &str,
    ) -> Result<()> {
        ensure!(self.peers.contains_key(&owner), "No such player");
        let outline = match area {
            Some((min, max)) => {
                let area = grid_box(min, max)?;
                let corner = |cells: [i32; 3]| {
                    std::array::from_fn(|a| cells[a] as f32 * crate::grid::CELL[a])
                };
                Some(Outline {
                    tool: tool.into(),
                    min: corner(area.min),
                    max: corner(area.max()),
                })
            }
            None => None,
        };
        self.notify(owner, Notice::SelectionBox(outline.map(Box::new)));
        Ok(())
    }

    /// The copy `owner` holds, if any.
    pub fn blueprint(&self, owner: OwnerId) -> Option<&Blueprint> {
        self.blueprints.get(&owner)
    }

    /// Place the copy `owner` holds with its pivot at `position` (snapped
    /// to the nearest stud corner and plate), turned `quarter_turns`,
    /// upside down if `flipped` and mirrored if `mirrored`. All of it is
    /// planted, or none of it, unless its Add-On asked for each brick that
    /// fits ([`HeldCopy::partial`]); one undo entry.
    pub(super) fn place_blueprint(
        &mut self,
        owner: OwnerId,
        position: [f32; 3],
        quarter_turns: u8,
        (mirrored, flipped): (bool, bool),
    ) -> Result<Reply> {
        ensure!(
            quarter_turns < 4
                && position
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 1_000_000.0),
            "Invalid copy placement"
        );
        // The player's pause since their last copy plant.
        let wait = self.plant_waits.get(&owner).copied().unwrap_or_default();
        let now = self.simulation.state().tick;
        if now < wait.next && self.blueprints.contains_key(&owner) {
            let left = (wait.next - now) as f32 / bri_world::TICKS_PER_SECOND as f32;
            let package = self.copies.get(&owner).map(|c| c.package.clone());
            return self.refuse_place(package, owner, CopyRefusal::Wait(left));
        }
        let blueprint = self
            .blueprints
            .get(&owner)
            .filter(|_| self.copies.get(&owner).is_none_or(|c| c.shown))
            .context("Select a build to copy first")?;
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        combat::ensure_may_build(
            &peer.combat,
            &self.minigames,
            bri_minigames::BuildAction::Build,
        )?;
        let tick = self.simulation.state().tick;
        let held = self.copies.get(&owner);
        let package = held.map(|c| c.package.clone());
        let plant_as = held.and_then(|c| c.plant_as.clone());
        let anchor = snap_anchor(position);
        let (bricks, inexact) = if mirrored || flipped {
            let (definitions, mirrors) = (&self.simulation.definitions, &mut self.mirrors);
            let (image, inexact) = blueprint.seen(flipped, mirrored, |id, reflection| {
                mirrors.image_in(definitions, id, reflection)
            });
            (image.placed(anchor, quarter_turns), inexact)
        } else {
            (blueprint.placed(anchor, quarter_turns), Default::default())
        };
        // The server's brick limit, then the plant rate: a copy needs a
        // plant window with room left and uses the rest of it.
        let settings = &self.admin.settings;
        if self.simulation.state().bricks.len() + bricks.len() > settings.brick_limit as usize
            || (!peer.actor.administrator && peer.plants >= settings.bricks_per_second)
        {
            return Err(crate::simulation::PlantFailure::Limit.into());
        }
        // TooFarDistance, from the feet to the copy's nearest part.
        let size = blueprint.turned_size(quarter_turns);
        let half = Vec3::new(size[0] as f32 * 0.25, 0.0, size[2] as f32 * 0.25);
        let middle = Vec3::from(anchor) + Vec3::new(0.0, size[1] as f32 * 0.1, 0.0);
        let feet = Vec3::from(peer.player.state().feet);
        if feet.distance(middle)
            > settings.too_far_distance.clamp(0.0, 100.0) + half.length() + size[1] as f32 * 0.1
        {
            return Err(crate::simulation::PlantFailure::TooFar.into());
        }
        // Another player's group needs their build trust still.
        let rate = settings.bricks_per_second;
        let mut actor = peer.actor.clone();
        if let Some(into) = plant_as {
            if !self.may_plant_into(owner, into.group, into.admin) {
                return self.refuse_place(package, owner, CopyRefusal::Group(into.name));
            }
            actor.owner = into.group;
        }
        let group = actor.owner;
        let total = bricks.len();
        let (partial, float) = match self.copies.get_mut(&owner) {
            Some(c) => {
                let once = c.float_once.take().is_some_and(|until| tick <= until);
                (c.partial, c.float || once)
            }
            None => (false, false),
        };
        let support = if float {
            crate::simulation::Support::Float
        } else {
            crate::simulation::Support::Required
        };
        let (planted, refused) = if partial {
            match self.simulation.plant_each(&actor, bricks, support) {
                (ids, refused) if !ids.is_empty() => (Ok(ids), refused),
                (_, mut refused) => {
                    let first = if refused.is_empty() {
                        anyhow::anyhow!("Nothing to plant")
                    } else {
                        refused.remove(0)
                    };
                    (Err(first), refused)
                }
            }
        } else {
            let planted = if float {
                self.simulation.plant_group_floating(&actor, bricks)
            } else {
                self.simulation.plant_group(&actor, bricks)
            };
            (planted, Vec::new())
        };
        if let Some(package) = &package {
            let count = planted.as_ref().map_or(0, Vec::len);
            let mut failures: Vec<&anyhow::Error> = planted.as_ref().err().into_iter().collect();
            failures.extend(&refused);
            self.report_place(package, owner, count, total, &failures, &inexact);
        }
        let ids = planted?;
        if let Some(peer) = self.peers.get_mut(&owner) {
            peer.plants = peer.plants.max(rate);
        }
        if let Some(wait) = self.plant_waits.get_mut(&owner) {
            wait.next = tick + wait.ticks;
        }
        for &id in &ids {
            self.special_planted(owner, id)?;
            self.dirty.insert(id);
        }
        let entry = undo::UndoEntry::Group {
            ids: ids.clone(),
            group,
        };
        self.push_copy_undo(owner, entry, package);
        self.cues
            .emit(tick, crate::presentation::CueKind::Plant, anchor);
        self.play_thread_three(tick, owner, "plant");
        Ok(Reply::Planted(ids[0]))
    }

    /// Refuse a copy's plant before any brick is tried: its Add-On hears
    /// why (`on_place`) and tells the player, else the player sees it.
    fn refuse_place(
        &mut self,
        package: Option<String>,
        owner: OwnerId,
        refusal: CopyRefusal,
    ) -> Result<Reply> {
        let error = anyhow::Error::new(refusal);
        if let Some(package) = package
            && self.report_place(&package, owner, 0, 0, &[&error], &Default::default())
        {
            return Ok(Reply::Accepted);
        }
        Err(error)
    }

    /// Put `item` in `owner`'s first free tool slot, unless they carry it
    /// already, and optionally take it in hand.
    ///
    /// Taking it in hand with every slot full puts the tool in hand (else
    /// the last) down on the ground to make room, where it can be picked up
    /// again: v20's `/duplicator` mounted its image without a slot, so it
    /// came out whatever the player carried.
    pub fn give_tool(&mut self, owner: OwnerId, item: &str, equip: bool) -> Result<()> {
        let actor = self
            .weapons
            .actor(bri_weapons::ActorId(owner))
            .context("Unknown connection")?;
        let held = actor
            .inventory
            .iter()
            .position(|held| held.as_deref() == Some(item));
        let full = actor.inventory.iter().all(Option::is_some);
        let room = actor.selected.unwrap_or(actor.inventory.len().saturating_sub(1));
        let direction = actor.frame.direction;
        let slot = match held {
            Some(slot) => slot,
            None => {
                if equip && full {
                    self.drop_tool(owner, room, direction)?;
                }
                self.give_item(owner, item)?
            }
        };
        if equip {
            self.equip_tool(owner, Some(slot))?;
        }
        Ok(())
    }

    /// An Add-On tool's `onFire`: run its command for the holder, aimed
    /// where the swing looks. A refusal (no such Add-On running, a
    /// cooldown) is only logged: the swing already played.
    pub(super) fn addon_tool_fire(&mut self, owner: OwnerId, command: &str) {
        self.addon_tool_command(owner, command, Vec::new());
    }

    /// [`Self::addon_tool_fire`] with arguments (a key's).
    pub(super) fn addon_tool_command(
        &mut self,
        owner: OwnerId,
        command: &str,
        args: Vec<super::packages::PackageArg>,
    ) {
        let Some(direction) = self
            .weapons
            .actor(bri_weapons::ActorId(owner))
            .map(|a| a.frame.direction.normalize_or_zero())
            .filter(|d| *d != Vec3::ZERO)
        else {
            return;
        };
        let Some((package, command)) = command.split_once(':') else {
            return;
        };
        let request = PackageCommand {
            package: package.into(),
            command: command.into(),
            args,
        };
        if let Err(error) = self.run_command(owner, request, direction, true) {
            if self.notices.len() == 64 {
                self.notices.pop_front();
            }
            self.notices
                .push_back(format!("Add-On tool {package}:{command}: {error:#}"));
        }
    }

    pub(super) fn forget_blueprint(&mut self, owner: OwnerId) {
        self.blueprints.remove(&owner);
        self.copies.remove(&owner);
    }
}

/// The grid cells covering the box from `min` to `max` (world units),
/// grown out to whole studs and plates.
pub(super) fn grid_box(min: [f32; 3], max: [f32; 3]) -> Result<crate::grid::Bounds> {
    let span = bri_package_runtime::ops::MAX_BOX_SPAN;
    ensure!(
        (0..3).all(|a| min[a].is_finite()
            && max[a].is_finite()
            && min[a].abs() <= 1_000_000.0
            && max[a].abs() <= 1_000_000.0
            && max[a] >= min[a]
            && max[a] - min[a] <= span),
        "A box is at most {span} units on a side"
    );
    // A thousandth of slack, so a corner on a grid line stays on it.
    let low: [i32; 3] =
        std::array::from_fn(|a| ((min[a] / crate::grid::CELL[a]) + 0.001).floor() as i32);
    let high: [i32; 3] =
        std::array::from_fn(|a| ((max[a] / crate::grid::CELL[a]) - 0.001).ceil() as i32);
    Ok(crate::grid::Bounds {
        min: low,
        size: std::array::from_fn(|a| (high[a] - low[a]).max(1)),
    })
}
