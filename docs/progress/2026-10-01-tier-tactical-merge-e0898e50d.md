# 2026-10-01 Tier+Tactical: main e0898e50d merged

Main (batch163, Slayer/CTF, Duplicators, grapples, Fill Can) merged into
the Tier+Tactical branch once. Where both sides had built the same thing,
one source stays:

- Player types: one converter (`addon-import/src/player_types.rs`), placed
  as main's are, in the import itself (`<ns>:archetype/<name>`, a shared
  package may carry archetypes). It keeps the Tier fields (`firstPersonOnly`,
  energy costs free at zero, script marks, a dependency's player type as
  base) and gains main's (`thirdPersonOnly`, `rideable`, `canRide`,
  `cameraMaxDist`, `jumpDelay`). The Tier companion for host content
  (`ports::Host`, `host_package`, the report's `host`) is gone. Tier 2's
  laid-down gunner is `weapon_package_tier2:archetype/lmgarmor`.
- Image `mount`/`unmount` commands: main's host-side check of each right
  hand's image from tick to tick runs them; the weapons runtime's own
  events for the hands are gone (an engine re-equip within a tick, as a
  loaded copy put in hand, no longer reads as the tool going away). The
  emote slot keeps its events, which the host check does not see.
- Port script bodies: main's (plain definitions first, packaged ones only
  where none, globals and whole files), still read without comments.
  The exec reach rule now also drops a script nothing execs from Slayer's
  stand-in, so its server.cs execs `Slayer_TeamSO.cs` as the real one does.
- `drop_item` data, `bottom_print`'s hide bar, `unmount_image`: main's.
- The pack shot check: the Tier one covers main's simpler one.

Checks: `cargo test` for addon-import, weapons, weapons-import,
package-runtime, motor, sim, net, events, client (lib, actor_effects), and
`cargo clippy --workspace --all-targets -- -D warnings`.
