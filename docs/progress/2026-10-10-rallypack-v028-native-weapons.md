# 2026-10-10 Rallypack native weapons playtest on v0.2.8

Maxwell authorized converting the remaining weapons for his own interactive
testing, explicitly declining exhaustive edge-case testing. Continued the
release-based `codex/rallypack-p90-v028` worktree; no engine change, visible
game launch, mouse input or gameplay automation. Original ZIPs/reference stay
read-only; original/generated assets are outside Git.

`tools/build_rallypack.py` packages all 45 Rallypack weapons and three ammo
items as `weapon_rallypack`, with shared native policy in the host companion
`weapon_rallypack-rules`. The disabled source debug `sese` item is excluded.
Rhai configurations are generated from recursively inherited original image
fields: magazine sizes, ammo identity, projectiles/speeds, recoil, pellet
counts/spread, reload/ADS image chains and sounds. Aiming follows authored
chains (including grenade-launcher and OICW modes), not a binary toggle.
Native rule template covers magazine reload, retained partial magazines,
reserve-fed load-one cycles, revolver dumping/refill, bow charging, grenade
priming/throw, resupply items, delayed distant sound, recoil shake and simple
shield frontal protection. Shield art uses a worn native image at the source
wrapper's mount offset; its appearance is explicitly for Maxwell to verify.

Applied the prior ordinary-surface material correction to weapons, shell and
projectile art, resolving placeholder textures from the ZIP and vanilla PNGs.
The final build reports no unresolved materials. Blast damage lived in custom
Torque functions rather than standard datablock fields; mapped its authored
damage/radius to the existing native blast primitive. Damage types are scoped
so the full pack and the standalone P90 coexist. Source temporary PlayerData
wrapper models are replaced by native policy, not exposed as new player types.

This is a broad playtest port, not complete script fidelity. Original blast
exposure/falloff/stun differs from native blasts. Penetration, ricochets,
suppression, programmable OICW airbursts, cooked grenade fuse preservation,
bipods, LMG heat/barrel policy, advanced sniper handling, revolver tricks,
armour and configurable source preferences remain absent or approximate.
Those assets remain available where converted, but unused mechanics are not
declared fully working. Ordinary/native projectile travel replaces scripted
stepped hitscan paths. No further engine hooks were added for this scope.

Maxwell reported the P90 rapidly glitching after exhausting ammunition.
Removed Ready's unconditional no-ammo transition in P90 and the full pack;
checks still run when attempting to fire. This prevents idle Ready → check →
dry/reload looping. Installed the corrected standalone P90 too.

Evidence/commands:

- Builder inputs: `E:\Downloads\Weapon_Rallypack.zip`, released
  `bri-import-addon.exe`, installed v0.2.8 content, designated vanilla v20.
  Final output `E:\Downloads\Rallypack-native-v028-ported`, 48 items and
  207 authored image configurations plus the native shield display image.
- `bri-addon-check` and a bounded `bri-addon-run --seconds 2` passed on the
  release-based runtime (earlier candidate before secondary-image addition).
- `cargo test -p bri-net --test rallypack -- --ignored --nocapture` with
  `BRI_RALLYPACK_TEST_ROOT` and `BRI_P90_CONTENT_ROOT`: ordinary mount/fire/
  reload paths passed for rifle, shotgun, sniper, revolver, rocket, bow,
  grenade and shield pistol. No script diagnostics. This is a family smoke
  check, not evidence for every original special mechanic or reload outcome.
- Focused `rallypack_p90 p90_exhausted` ignored test passed: use all rounds and
  spare magazines, release trigger, wait, assert zero ammo/no spares, no reload
  and stable Ready state. The earlier firing/damage/network checks are recorded
  in the P90 entry, not repeated exhaustively here.
- Released `bri-client.exe --check` with both ports enabled in an independent
  package-selection snapshot: exit 0; 14 maps, 988 brick definitions, 35 save
  pictures, 14 lighting maps. No P90/Rallypack health faults; only two existing
  Butterfly Knife warnings. Snapshot has real directories and hard-linked
  immutable content, with independent package lists and client state. The
  running user's game and live Add-On selections were not changed.
- Packaging checks caught/fixed stale debug-item physics, unscoped damage-type
  conflicts, the native shield presentation's missing evidence, and unused
  source wrapper body-art discovery. An intermediate snapshot lacked a newly
  added base sound; added that file to the snapshot before its final check.
- Worn images use slot 2 (slot 1 is reserved); the smoke check caught and fixed
  the initial attempted slot 1. Exported source spray seeds may be negative;
  their decimal digits now use absolute magnitude for the native pattern.

Installed both full-pack folders under the requested release `content/addons`.
Saved `E:\Downloads\Rallypack-Weapons-native-v028.zip` with both full-pack
folders, updated P90 folders and usage/limitations. Maxwell restarts, enables
Rallypack Weapons, chooses weapons in minigame loadout/item spawn bricks, and
performs the visual/gameplay/internet join check. User owns all interactive
acceptance. No push/main integration or alpha completion is claimed.
