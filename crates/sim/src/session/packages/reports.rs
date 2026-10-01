//! Score reports Add-On rules show players (`show_report`), with the
//! columns Add-Ons changed for a game (`report_column`) put in when the
//! report is sent: at the end of the tick's package work, so Capture the
//! Flag's columns land in Slayer's report whichever runs its round-end
//! hook first.
use super::*;
use bri_package_runtime::report::{ColumnChange, MAX_REPORT_OVERRIDES, Report};

#[derive(Default)]
pub(in crate::session) struct Reports {
    /// Column changes by game, oldest first, one per column key.
    columns: BTreeMap<u64, Vec<ColumnChange>>,
    /// Reports to send each player this tick (the last asked wins), or
    /// `None` to close theirs.
    pending: BTreeMap<OwnerId, Option<Box<Report>>>,
}
impl Reports {
    /// A game is gone: so are its column changes.
    pub(in crate::session) fn forget_game(&mut self, game: u64) {
        self.columns.remove(&game);
    }
}

impl Session {
    pub(in crate::session) fn package_show_report(
        &mut self,
        package: &str,
        player: OwnerId,
        report: Option<Box<Report>>,
    ) -> Result<()> {
        ensure!(self.peers.contains_key(&player), "No such player");
        self.take_cue(package)?;
        let host = self.packages.as_mut().context("No packages are enabled")?;
        host.reports.pending.insert(player, report);
        Ok(())
    }
    pub(in crate::session) fn package_report_column(
        &mut self,
        game: u64,
        change: ColumnChange,
    ) -> Result<()> {
        ensure!(
            self.minigames.game(bri_minigames::GameId(game)).is_ok(),
            "No mini-game {game}"
        );
        let host = self.packages.as_mut().context("No packages are enabled")?;
        let changes = host.reports.columns.entry(game).or_default();
        changes.retain(|c| c.key != change.key);
        ensure!(
            changes.len() < MAX_REPORT_OVERRIDES,
            "a game's report changes at most {MAX_REPORT_OVERRIDES} columns"
        );
        changes.push(change);
        Ok(())
    }
    /// Send the reports asked for, each with its player's game's columns.
    pub(in crate::session) fn flush_reports(&mut self) {
        let Some(host) = self.packages.as_mut() else {
            return;
        };
        if host.reports.pending.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut host.reports.pending);
        for (owner, mut report) in pending {
            if let Some(report) = report.as_mut()
                && let Some(game) = self.game_of(owner)
                && let Some(changes) = self
                    .packages
                    .as_ref()
                    .and_then(|h| h.reports.columns.get(&game.0))
            {
                report.apply(changes);
            }
            self.notify(owner, Notice::Report(report));
        }
    }
}
