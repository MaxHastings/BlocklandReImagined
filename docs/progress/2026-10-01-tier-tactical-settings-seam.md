# 2026-10-01 Tier+Tactical: RTB preferences as server settings

## What changed
- Add-On settings gain a `server` scope: one value for the whole server,
  changed only by the host (Admin menu, new "Add-On Settings" button) and
  saved with the host's prefs as `$Pref::Server::AddOn::<ns>::<key>`. Stored
  values for keys no running Add-On declares are kept; invalid ones read as
  the default. Host Configure validates them against the declarations.
- A server setting may name the v20 global it stands for (`global`,
  `$Pref::Server::...`). Rules read it with `pref("$Pref::Server::TT::X")`
  (`()` when no running Add-On declares it) or `server_setting(key)`.
- Importer: `RTB_registerPref` calls become server settings (`rtb.rs`) when
  the port's rules opt in through `rules.prefs`; the report lists them as
  ported, and the rest stay unsupported with "keeps its default".
  `needsRestart=1` prefs stay unsupported. `isFunction(x)` is classified as
  ambiguous (absent unless the Add-On defines it), so the RTB branch is the
  one played and `TT_defaultIfUnset` never runs.
- Tier rules read the Start*/Max* ammo, endless ammo/grenades, drop and
  pickup prefs and LMGSlow; Medic1 reads MedicHealBots/MedicHealEnemy and
  checks teams (same minigame, same or allied team) like `TT_canHeal`.
- Also: SMG reload-wait handles marked never-run; Impact Rifle sha256 pinned.

## Evidence
- `cargo test -p bri-addon-import` (tier_port: `tier_preferences_are_server_settings_the_host_changes`,
  `medic1_heals_its_own_team_unless_the_host_lets_it_heal_others`; rtb unit test).
- `cargo test -p bri-ui` (admin prefs round trip, server Add-On settings screen).
- Real copies: Tier1 unsupported 38 -> 15, Medic1 0, Tier2 0 (LMGSlow ported),
  Explosive1 6, Melee II 3 (shield prefs).

## Remaining
- Prefs that change engine weapon data (Ammo system, DisplayAmmo, Recoil,
  bullet slow, nade displays, shield prefs) stay at their defaults: they need
  a seam binding weapon data fields to server settings.
