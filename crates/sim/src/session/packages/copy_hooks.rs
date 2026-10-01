//! A duplicator Add-On hearing how its copies went: `on_copy` after it
//! took one, `on_place` after the player planted it. An Add-On that
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
fn plant_error(error: &anyhow::Error) -> &'static str {
    use crate::simulation::PlantFailure;
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
            info.insert("total".into(), (outcome.total as i64).into());
            info.insert("limit_reached".into(), outcome.limit_reached.into());
            info.insert("refused".into(), (outcome.refused as i64).into());
            let (error, message) = match &outcome.error {
                Some((code, message)) => (Dynamic::from(code.to_string()), message.clone()),
                None => (Dynamic::UNIT, String::new()),
            };
            info.insert("error".into(), error);
            info.insert("message".into(), message.into());
            // The held copy's grid size, for a duplicator to show.
            let size = match (&outcome.error, self.blueprints.get(&player)) {
                (None, Some(copy)) if outcome.action != "save" => Dynamic::from_array(
                    copy.size.iter().map(|n| Dynamic::from_int(i64::from(*n))).collect(),
                ),
                _ => Dynamic::UNIT,
            };
            info.insert("size".into(), size);
            self.queue_report(package, "on_copy", player, info);
            return;
        }
        if let Some((_, message)) = outcome.error {
            self.center_print(player, message);
            return;
        }
        let name = outcome.name.unwrap_or_default();
        match outcome.action {
            "save" => self.center_print(player, format!("Saved the copy as '{name}'.")),
            "cut" => self.bottom_count(player, "Cut", outcome.bricks),
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

    /// Tell the Add-On whose copy `player` planted how it went, with why
    /// each brick left out was refused. False when it has no `on_place`, so
    /// the engine speaks instead.
    pub(in crate::session) fn report_place(
        &mut self,
        package: &str,
        player: OwnerId,
        planted: usize,
        bricks: usize,
        failures: &[&anyhow::Error],
    ) -> bool {
        if !self.declares(package, |b| b.on_place) {
            return false;
        }
        let error = failures.first().copied();
        let mut info = Map::new();
        info.insert("planted".into(), (planted as i64).into());
        info.insert("bricks".into(), (bricks as i64).into());
        // How many bricks each plant error kept out.
        let mut failed = Map::new();
        for failure in failures {
            let count = failed
                .entry(plant_error(failure).into())
                .or_insert_with(|| Dynamic::from_int(0));
            *count = Dynamic::from_int(count.as_int().unwrap_or(0) + 1);
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
        self.queue_report(package, "on_place", player, info);
        true
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
