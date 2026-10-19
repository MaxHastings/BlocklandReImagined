# 2026-10-18 Weapons: held-fire error, a reload from empty, longer hitscan

Three generic additions to `crates/weapons`, made for a game's pistol and sniper rifle and
named for neither. Each has its own commit.

- `shot.error` (`3b2f4200c`): a spread that builds as the holder keeps firing and settles
  when they stop. The holder's error rises by `1 / rise_ticks` a tick while the trigger is
  held or a shot left within `recover_ticks`, falls by `1 / fall_ticks` otherwise, 0 to 1;
  a shot's spread is `min` at 0 and `max` at 1. It replaces `spread` and the moving,
  crouched, airborne and rested variants. It is host weapon state kept with the actor, like the
  magazine. Tests: `tests/shot_error.rs` (a held burst widens, a rest settles it, a tap
  leaves it part risen, validation).
- `hitscan.range` to 8192 (`f5289ce42`), for a rifle that reaches a kilometre of its
  source's world; a test pins 4120 valid and 9000 refused. Docs describe `shot.error` too.
- `magazine.empty_reload_ticks`, `empty_reload_image_sequence`, `empty_reload_sound`
  (`fa715053c`): a reload that starts with nothing loaded takes these in place of the
  ordinary ones. Test: `tests/magazines.rs`.

Evidence and decisions are in the Add-Ons repository's `halo/PISTOL-SNIPER-HANDOFF.md`.
Checks run: `cargo test -j 2 -p bri-weapons` for the two test files; the tier checks below.

Open (not built, found while converting): zoom is each holder's own presentation, so a rule
that depends on it (an error only unzoomed, an aim assist only zoomed) cannot be expressed;
it needs an authoritative zoom bit on the wire. A flown projectile at a rifle's speed
(4120 units a second) exists but gives up lag compensation; that choice is left to the
weapon's owner.
