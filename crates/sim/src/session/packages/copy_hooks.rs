//! A duplicator Add-On hearing how its copies went: `on_copy` after it
//! took one, `on_place` after the player planted it, `on_copy_ghost` as
//! the player moves it. An Add-On that
//! declares the hook says what it likes to the player; for one that does
//! not, the engine says it plainly.
use super::*;
use crate::session::copy_store::CopyOutcome;
use bri_package_runtime::rhai::Map;

/// Reports waiting for the start of the next tick, oldest first.
const MAX_PENDING: usize = 256;

/// A report for one package's hook.
struct Report {
    package: String,
    hook: &'static str,
    player: OwnerId,
    info: Map,
}

#[derive(Default)]
pub(in crate::session) struct CopyHooks {
    reports: VecDeque<Report>,
}

/// v20's plant error a failed plant is reported as.
pub(in crate::session) fn plant_error(error: &anyhow::Error) -> &'static str {
    use crate::session::blueprints::CopyRefusal;
    use crate::simulation::PlantFailure;
    match error.downcast_ref::<CopyRefusal>() {
        Some(CopyRefusal::Wait(_)) => return "wait",
        Some(CopyRefusal::Group(_)) => return "group",
        None => {}
    }
    match error.downcast_ref::<PlantFailure>() {
        Some(PlantFailure::Overlap) => "overlap",
        Some(PlantFailure::Float) => "float",
        Some(PlantFailure::Buried) => "buried",
        Some(PlantFailure::Stuck) => "stuck",
        Some(PlantFailure::TooFar) => "too_far",
        Some(PlantFailure::Limit) => "limit",
        Some(PlantFailure::Forbidden) => "forbidden",
        None => "other",
    }
}

impl Session {
    fn declares(
        &self,
        package: &str,
        hook: fn(&bri_package_runtime::content::Behaviour) -> bool,
    ) -> bool {
        self.packages.as_ref().is_some_and(|host| {
            host.catalog
                .packages
                .get(package)
                .and_then(|p| p.behaviour.as_ref())
                .is_some_and(hook)
        })
    }

    fn queue_report(&mut self, package: &str, hook: &'static str, player: OwnerId, info: Map) {
        let Some(host) = self.packages.as_mut() else {
            return;
        };
        if host.copy_hooks.reports.len() == MAX_PENDING {
            note(
                host,
                Diagnostic::warning(
                    "hook.dropped",
                    format!("Too many copies in one tick for {hook}"),
                ),
            );
            return;
        }
        host.copy_hooks.reports.push_back(Report {
            package: package.into(),
            hook,
            player,
            info,
        });
    }

    /// Tell `package` (or else `player`) what its copy, save or load did.
    pub(in crate::session) fn report_copy(
        &mut self,
        package: &str,
        player: OwnerId,
        outcome: impl Into<CopyOutcome>,
    ) {
        let outcome = outcome.into();
        if self.declares(package, |b| b.on_copy) {
            let mut info = Map::new();
            info.insert("action".into(), outcome.action.into());
            info.insert(
                "name".into(),
                outcome.name.clone().map_or(Dynamic::UNIT, Dynamic::from),
            );
            info.insert("bricks".into(), (outcome.bricks as i64).into());
            info.insert("placed".into(), (outcome.placed as i64).into());
            info.insert("total".into(), (outcome.total as i64).into());
            info.insert("limit_reached".into(), outcome.limit_reached.into());
            info.insert("refused".into(), (outcome.refused as i64).into());
            info.insert("working".into(), outcome.working.into());
            // While working: bricks found still to look around, and how far
            // a search has got in percent (-1: not searching).
            info.insert("queued".into(), (outcome.queued as i64).into());
            let searched = outcome.searched.map_or(-1, |percent| percent as i64);
            info.insert("searched".into(), searched.into());
            info.insert(
                "names".into(),
                Dynamic::from_array(outcome.names.iter().cloned().map(Dynamic::from).collect()),
            );
            let (error, message) = match &outcome.error {
                Some((code, message)) => (Dynamic::from(code.to_string()), message.clone()),
                None => (Dynamic::UNIT, String::new()),
            };
            info.insert("error".into(), error);
            info.insert("message".into(), message.into());
            // The held copy's grid size, for a duplicator to show, and how
            // many of its bricks its player sees as the ghost.
            let held = match (&outcome.error, self.blueprints.get(&player)) {
                (None, Some(copy)) if outcome.action != "save" && !outcome.working => Some(copy),
                _ => None,
            };
            let size = held.map_or(Dynamic::UNIT, |copy| {
                Dynamic::from_array(
                    copy.size.iter().map(|n| Dynamic::from_int(i64::from(*n))).collect(),
                )
            });
            info.insert("size".into(), size);
            let ghosted = held.map_or(0, |copy| copy.len().min(crate::blueprint::MAX_GHOST_BRICKS));
            info.insert("ghosted".into(), (ghosted as i64).into());
            // Where the bricks a selection took stand: the box round them
            // all, for a duplicator to turn into a selection box.
            let area = match (&outcome.error, self.copies.get(&player)) {
                (None, Some(held)) if outcome.action == "select" && !outcome.working => held.area,
                _ => None,
            };
            let point = |p: glam::Vec3| {
                Dynamic::from_array(p.to_array().map(|v| Dynamic::from_float(f64::from(v))).to_vec())
            };
            info.insert(
                "box".into(),
                area.map_or(Dynamic::UNIT, |(min, max)| {
                    let mut corners = Map::new();
                    corners.insert("min".into(), point(min));
                    corners.insert("max".into(), point(max));
                    corners.into()
                }),
            );
            self.queue_report(package, "on_copy", player, info);
            return;
        }
        if let Some((_, message)) = outcome.error {
            self.center_print(player, message);
            return;
        }
        if outcome.working {
            let percent = (outcome.bricks * 100).checked_div(outcome.total).unwrap_or(0);
            let text = match outcome.total {
                0 => format!("Working... ({} bricks)", outcome.bricks),
                _ => format!("Working... ({percent}%)"),
            };
            self.notify(
                player,
                Notice::Bottom {
                    text,
                    seconds: 1.0,
                    hide_bar: false,
                },
            );
            return;
        }
        let name = outcome.name.unwrap_or_default();
        match outcome.action {
            "save" => self.center_print(player, format!("Saved the copy as '{name}'.")),
            "undo" => self.center_print(
                player,
                format!(
                    "Next undo will affect {} bricks. Press undo again to continue.",
                    outcome.bricks
                ),
            ),
            "plant_as" if !name.is_empty() => {
                self.center_print(player, format!("Your copies now go into {name}'s bricks."))
            }
            "plant_as" => self.center_print(player, "Your copies go into your own bricks.".into()),
            "list" if outcome.names.is_empty() => {
                self.center_print(player, "No copies are saved here.".into())
            }
            "list" => {
                for line in &outcome.names {
                    self.notify(player, Notice::Chat(line.clone()));
                }
            }
            "cut" | "supercut" => self.bottom_count(player, "Cut", outcome.bricks),
            "paint" => self.bottom_count(player, "Painted", outcome.bricks),
            "wrench" => self.bottom_count(player, "Wrenched", outcome.bricks),
            "fill" => self.bottom_count(player, "Filled in", outcome.bricks),
            _ => {
                let verb = if outcome.action == "load" { "Loaded" } else { "Copied" };
                self.bottom_count(player, verb, outcome.bricks);
                if outcome.limit_reached {
                    self.center_print(
                        player,
                        format!(
                            "That build was too big: only {} of its {} bricks were taken.",
                            outcome.bricks, outcome.total
                        ),
                    );
                }
            }
        }
    }

    /// Tell the Add-On whose copy `player` planted how it went (`planted`
    /// of `bricks`, and whether the player cancelled it part way), with why
    /// each brick left out was refused. False when it has no `on_place`,
    /// so the engine speaks instead.
    pub(in crate::session) fn report_place(
        &mut self,
        package: &str,
        player: OwnerId,
        (planted, bricks, canceled, float_refused): (usize, usize, bool, bool),
        refused: &crate::session::blueprints::Refusals,
        inexact: &crate::blueprint::Inexact,
    ) -> bool {
        if !self.declares(package, |b| b.on_place) {
            return false;
        }
        let error = refused.first.as_ref();
        let mut info = Map::new();
        info.insert("planted".into(), (planted as i64).into());
        info.insert("bricks".into(), (bricks as i64).into());
        info.insert("canceled".into(), canceled.into());
        info.insert("float_refused".into(), float_refused.into());
        // How many bricks each plant error kept out.
        let mut failed = Map::new();
        for (code, count) in &refused.by_error {
            failed.insert((*code).into(), Dynamic::from_int(*count as i64));
        }
        info.insert("failed".into(), failed.into());
        info.insert(
            "error".into(),
            error.map_or(Dynamic::UNIT, |e| Dynamic::from(plant_error(e).to_string())),
        );
        info.insert(
            "message".into(),
            error.map_or(String::new(), |e| format!("{e:#}")).into(),
        );
        let wait = error.and_then(|e| match e.downcast_ref() {
            Some(crate::session::blueprints::CopyRefusal::Wait(seconds)) => Some(*seconds),
            _ => None,
        });
        info.insert(
            "wait".into(),
            wait.map_or(Dynamic::UNIT, |s| Dynamic::from_float(f64::from(s))),
        );
        // The bricks a mirrored or upside-down plant had no exact image
        // for, as the build menu names them.
        let names = |ids: &[String]| -> Dynamic {
            let catalog = &self.tool_catalog.brick_names;
            Dynamic::from_array(
                ids.iter()
                    .map(|id| Dynamic::from(catalog.get(id).unwrap_or(id).clone()))
                    .collect(),
            )
        };
        let mut mirror_errors = Map::new();
        mirror_errors.insert("side".into(), names(&inexact.side));
        mirror_errors.insert("upside_down".into(), names(&inexact.upside_down));
        info.insert("mirror_errors".into(), mirror_errors.into());
        self.queue_report(package, "on_place", player, info);
        true
    }

    /// Tell `package` where the copy `player` places stands now: the box
    /// round it (`#{ min, max }`), or `()` once it is gone.
    pub(in crate::session) fn report_copy_ghost(
        &mut self,
        package: &str,
        player: OwnerId,
        area: Option<([f32; 3], [f32; 3])>,
    ) {
        if !self.declares(package, |b| b.on_copy_ghost) {
            return;
        }
        let point = |p: [f32; 3]| {
            Dynamic::from_array(p.map(|v| Dynamic::from_float(f64::from(v))).to_vec())
        };
        let mut info = Map::new();
        info.insert(
            "box".into(),
            area.map_or(Dynamic::UNIT, |(min, max)| {
                let mut corners = Map::new();
                corners.insert("min".into(), point(min));
                corners.insert("max".into(), point(max));
                corners.into()
            }),
        );
        self.queue_report(package, "on_copy_ghost", player, info);
    }

    /// `on_copy` and `on_place` for each report since the last tick. Each
    /// answers the player's own command, so the hook acts for them as that
    /// command did (their copy lit, moved, put away).
    pub(super) fn deliver_copy_reports(&mut self) {
        let Some(host) = self.packages.as_mut() else {
            return;
        };
        let reports = std::mem::take(&mut host.copy_hooks.reports);
        for report in reports {
            let _ = self.run_package(
                &report.package,
                report.hook,
                vec![Dynamic::from_int(report.player as i64), report.info.into()],
                Budget::Command,
                Some(report.player),
                None,
                None,
            );
            self.charge_work(&report.package);
        }
    }
}
