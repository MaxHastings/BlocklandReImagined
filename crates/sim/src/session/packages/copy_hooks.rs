//! A duplicator Add-On hearing how its copies went: `on_copy` after it
//! took one, `on_place` after the player planted it. An Add-On that
//! declares the hook says what it likes to the player; for one that does
//! not, the engine says it plainly.
use super::*;
use crate::session::blueprints::Copied;
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

    /// Tell `package` (or else `player`) what its copy took.
    pub(super) fn report_copy(&mut self, package: &str, player: OwnerId, copied: Copied) {
        let selection = &copied.selection;
        if self.declares(package, |b| b.on_copy) {
            let mut info = Map::new();
            info.insert("bricks".into(), (selection.bricks.len() as i64).into());
            info.insert("limit_reached".into(), selection.limit_reached.into());
            info.insert("refused".into(), (selection.refused as i64).into());
            let (error, message) = match &copied.error {
                Some((code, message)) => (Dynamic::from(code.to_string()), message.clone()),
                None => (Dynamic::UNIT, String::new()),
            };
            info.insert("error".into(), error);
            info.insert("message".into(), message.into());
            self.queue_report(package, "on_copy", player, info);
            return;
        }
        match copied.error {
            Some((_, message)) => self.center_print(player, message),
            None => {
                self.bottom_count(player, "Copied", selection.bricks.len());
                if selection.limit_reached {
                    self.center_print(
                        player,
                        format!(
                            "That build was too big: only {} bricks were copied.",
                            selection.bricks.len()
                        ),
                    );
                }
            }
        }
    }

    /// Tell the Add-On whose copy `player` planted how it went. False when
    /// it has no `on_place`, so the engine speaks instead.
    pub(in crate::session) fn report_place(
        &mut self,
        package: &str,
        player: OwnerId,
        planted: usize,
        bricks: usize,
        error: Option<&anyhow::Error>,
    ) -> bool {
        if !self.declares(package, |b| b.on_place) {
            return false;
        }
        let mut info = Map::new();
        info.insert("planted".into(), (planted as i64).into());
        info.insert("bricks".into(), (bricks as i64).into());
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

    /// `on_copy` and `on_place` for each report since the last tick.
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
                None,
                None,
                None,
            );
            self.charge_work(&report.package);
        }
    }
}
