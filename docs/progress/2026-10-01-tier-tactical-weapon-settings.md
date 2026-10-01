# 2026-10-01 Tier+Tactical: weapon fields from server settings

The coordinator asked for a generic seam binding weapon data to server
settings so Tier's preferences stop sitting at their defaults, and for
restart-only preferences to apply at the next start instead of staying
unsupported.

What changed:

- `bri_weapons::Binding` / `Pack::bindings`: a server setting (by
  `<package>:<key>` or its v20 global) sets a field of the pack's items,
  images or projectiles, by value or a number scaled, with `when` guards.
  `Pack::with_settings` derives the pack; the result is validated as any
  pack and must keep the same definitions and state counts, so a value out
  of a field's range is refused when the host sets it.
- The session keeps the authored pack, derives the played one from the
  server settings at start, on `HostConfigure` and at a map load, and
  `WeaponsWorld::retune` swaps it keeping every holder's rounds and
  reserves. `Checkpoint`/`Delta::weapon_settings` send the values; the
  client derives the same pack from its authored one.
- Magazine `supply` (reserve, endless, unlimited, counted, both) and
  `hide_display`: Tier's four ammo systems and its always-reload guns.
- Importer: a port's `rules.settings` become bindings.
  `_shared/tier-tactical` binds the Ammo System, Always Reload (Ex/NonEx,
  by each item's `TT_alwaysReloadPref` or `TT_alwaysReload`), Display Ammo,
  Display Duration, Recoil and Disable Bullet Slowdown.
- Restart settings: `SettingDef::restart`, from RTB's needsRestart (also
  split out alone as `claude/server-settings-3onhik` for New Duplicator).

Evidence on the real copies (`run4.sh`): Tier 1 unsupported 15 → 8 (88/88
behaviours), Tier 2 0, Tier 1A 0, Short Rifle 0, Explosive 1 still 6.
Tests: `crates/weapons/tests/settings.rs` (9), tier_port
`tier_preferences_are_server_settings_the_host_changes` (the host switches
T+T2 → Classic → Arena and Recoil off in a game), `sim/tests/server_settings.rs`.

Next: the rest of Tier's preferences (Remount Duplicates, the inventory
bugfixes, the death preferences, the grenade and molotov preferences, the
shield preferences) and the restart ones (Disable Ammo Pickups, the easter
egg, Disable Tier 1 / Explosive 1, Disable Nade Pickups); then
Event_AddAmmoTT and the Frogs packs.
