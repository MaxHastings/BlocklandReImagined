//! The weapons pack's fields server settings decide
//! ([`bri_weapons::Binding`]): the host plays its authored pack with the
//! running Add-Ons' server settings applied, derived again as the host
//! changes one, and players derive the same from the values replicated
//! with the world ([`Session::weapon_settings`]).
use super::*;
use bri_package::setting::SettingValue;

impl Session {
    /// The value, as text, of the server setting a binding names:
    /// `<package>:<key>`, or the v20 global its setting stands for. One the
    /// game reads only as it starts has the value it started with.
    fn bound_setting(&self, name: &str, stored: &BTreeMap<String, SettingValue>) -> Option<String> {
        let host = self.packages.as_ref()?;
        let s = if name.starts_with('$') {
            host.settings.by_global(name)?
        } else {
            host.settings.get(name)?
        };
        if s.def.scope != bri_package::setting::SettingScope::Server {
            return None;
        }
        let key = s.key();
        let value = if s.def.restart {
            self.started_settings.get(&key)
        } else {
            stored.get(&key)
        };
        let value = value
            .filter(|v| s.check(v).is_ok())
            .unwrap_or(&s.def.default);
        Some(value.to_string())
    }

    /// The values, as text, of every setting the weapons pack's bindings
    /// read that a running Add-On declares, by the name the binding uses:
    /// what a player needs to derive the pack the host plays.
    pub fn weapon_settings(&self) -> &BTreeMap<String, String> {
        &self.weapon_values
    }

    /// [`Self::weapon_settings`] were the server settings `stored`.
    fn bound_values(&self, stored: &BTreeMap<String, SettingValue>) -> BTreeMap<String, String> {
        self.authored_weapons
            .bound_settings()
            .into_iter()
            .filter_map(|name| Some((name.to_owned(), self.bound_setting(name, stored)?)))
            .collect()
    }

    /// Refuses the host's new server settings when they would take a
    /// weapon's field out of its range.
    pub(super) fn check_weapon_settings(&self, stored: &BTreeMap<String, SettingValue>) -> Result<()> {
        let values = self.bound_values(stored);
        self.authored_weapons
            .with_settings(|name| values.get(name).cloned())
            .map(drop)
    }

    /// Plays the pack the current server settings make of the authored one.
    pub(super) fn retune_weapons(&mut self) -> Result<()> {
        if self.authored_weapons.bindings.is_empty() {
            return Ok(());
        }
        let values = self.bound_values(&self.admin.settings.addon_settings);
        if values == self.weapon_values {
            return Ok(());
        }
        let pack = self
            .authored_weapons
            .with_settings(|name| values.get(name).cloned())?;
        // Items a setting shows or hides come and go from loadouts; the
        // player types and Add-On limits the catalog was given stay.
        let mut catalog = self.minigames.catalog().clone();
        catalog.items = super::combat::catalog(&pack).items;
        self.weapons.retune(pack)?;
        self.weapon_values = values;
        if &catalog != self.minigames.catalog() {
            self.minigames
                .set_catalog(catalog)
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        }
        Ok(())
    }

    /// As the server starts or loads a map with its Add-Ons: the settings
    /// it reads only then take the values they have now
    /// ([`Self::start_settings`]), and the pack is derived. Values it
    /// cannot take leave the authored fields, with a note for the host.
    pub(super) fn start_weapon_settings(&mut self) {
        self.start_settings();
        if let Err(error) = self.retune_weapons()
            && let Some(host) = self.packages.as_mut()
        {
            packages::note(
                host,
                bri_package_runtime::Diagnostic::warning(
                    "weapons.settings",
                    format!("The weapons keep their own values: {error:#}"),
                ),
            );
        }
    }
}
