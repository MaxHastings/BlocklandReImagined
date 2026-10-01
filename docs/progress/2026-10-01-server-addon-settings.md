# 2026-10-01 Server-wide Add-On settings

Split from the Tier+Tactical lane so it can land ahead of Tier and
Adventure: New Duplicator's `$Pref::Server::ND::*` preferences apply in free
build, outside any mini-game, and need it.

What changed:

- `SettingScope::Server`: one value for the whole server. Only the host
  changes it, in the Admin menu's new Add-On Settings button (the same
  window as a mini-game's settings, showing the server's). The values go
  with the host's Server Settings (`ServerSettings::addon_settings`,
  checked against the running Add-Ons' definitions on `HostConfigure`) and
  save as `$Pref::Server::AddOn::<namespace>::<key>`. Values of an Add-On
  that is off are kept for when it runs again.
- `SettingDef::global`: the v20 global a server setting stands for. Rules
  read it with `pref("$Pref::Server::...")` from any Add-On, whichever one
  declares it (`()` when none does, as an unset global), or by key with
  `server_setting(key)`.
- `SettingDef::restart` (RTB's needsRestart): the game reads it only as the
  server starts or loads a map. It keeps the value it had then; the host's
  change is kept and applies at the next start or map change. The window
  marks such settings with `*` and a note.
- Wire: `crates/net/protocol-changes/server-addon-settings.md`.

Left on the Tier branch: the importer turning a copy's `RTB_registerPref`
calls into these settings, and the Tier and Medic ports that read them.

Evidence: `cargo test -p bri-sim --test server_settings` (the host sets a
value, a value out of range is refused, a restart setting waits for a new
server and for a map change), `cargo test -p bri-ui --test minigame_screens`
(the Admin menu opens the server's settings, the restart mark and note),
`cargo test -p bri-ui models::admin` (saved as prefs and read back).
