//! Panels (`show_panel`, `on_panel`): windows an Add-On's rules show a
//! player, built from the game's own widgets, and what the player does in
//! them. The host keeps the panel each player has open, so an event is
//! only ever one that panel could send (`Panel::accepts`), and only the
//! Add-On that showed it hears it.
use super::*;
use bri_package_runtime::panel::{Panel, PanelEvent};
use bri_package_runtime::rhai::Map;

/// A panel shown to a player, and whose rules showed it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelShown {
    pub package: String,
    pub panel: Panel,
}

/// What a player did in their panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelCommand {
    pub package: String,
    pub event: PanelEvent,
}

impl Session {
    pub(in crate::session) fn package_show_panel(
        &mut self,
        package: &str,
        player: OwnerId,
        panel: Panel,
    ) -> Result<()> {
        ensure!(self.peers.contains_key(&player), "No such player");
        ensure!(!self.bots.is_bot(player), "Bots see no panels");
        if let Some(problem) = panel.problem() {
            anyhow::bail!("{problem}");
        }
        self.take_cue(package)?;
        let shown = PanelShown {
            package: package.to_owned(),
            panel,
        };
        let host = self.packages.as_mut().context("No packages are enabled")?;
        host.panels.insert(player, shown.clone());
        self.notify(player, Notice::Panel(Some(Box::new(shown))));
        Ok(())
    }

    pub(in crate::session) fn package_close_panel(
        &mut self,
        package: &str,
        player: OwnerId,
        id: &str,
    ) -> Result<()> {
        let host = self.packages.as_mut().context("No packages are enabled")?;
        let open = host
            .panels
            .get(&player)
            .is_some_and(|p| p.package == package && p.panel.id == id);
        if open {
            host.panels.remove(&player);
            self.notify(player, Notice::Panel(None));
        }
        Ok(())
    }

    /// A player left: their panel goes with them.
    pub(in crate::session) fn forget_panel(&mut self, player: OwnerId) {
        if let Some(host) = self.packages.as_mut() {
            host.panels.remove(&player);
        }
    }

    /// The panel `player` has open, if any.
    pub fn open_panel(&self, player: OwnerId) -> Option<&PanelShown> {
        self.packages.as_ref()?.panels.get(&player)
    }

    /// A player pressed a button, picked a list row or closed their panel:
    /// its Add-On hears it (`on_panel`) when the panel could have sent it.
    pub(in crate::session) fn panel_command(
        &mut self,
        owner: OwnerId,
        request: PanelCommand,
    ) -> Result<()> {
        let PanelCommand { package, event } = request;
        let host = self.packages.as_mut().context("No packages are enabled")?;
        let shown = host
            .panels
            .get(&owner)
            .filter(|p| p.package == package)
            .context("That panel is not open")?;
        ensure!(
            shown.panel.accepts(&event),
            "The panel has no such control or value"
        );
        if event.control == "close" {
            host.panels.remove(&owner);
        }
        let hears = host
            .catalog
            .packages
            .get(&package)
            .and_then(|p| p.behaviour.as_ref())
            .is_some_and(|b| b.on_panel);
        if !hears {
            return Ok(());
        }
        let mut map = Map::new();
        map.insert("panel".into(), event.panel.into());
        map.insert("control".into(), event.control.into());
        let fields: Map = event
            .fields
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        let lists: Map = event
            .lists
            .into_iter()
            .map(|(k, v)| {
                let row = v.map_or(Dynamic::UNIT, |r| Dynamic::from_int(r.into()));
                (k.into(), row)
            })
            .collect();
        map.insert("fields".into(), Dynamic::from_map(fields));
        map.insert("lists".into(), Dynamic::from_map(lists));
        let _ = self.run_package(
            &package,
            "on_panel",
            vec![Dynamic::from_int(owner as i64), Dynamic::from_map(map)],
            Budget::Command,
            Some(owner),
            None,
            None,
        );
        self.charge_work(&package);
        Ok(())
    }
}
