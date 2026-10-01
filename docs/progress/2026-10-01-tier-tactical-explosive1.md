# 2026-10-01 Tier+Tactical: Explosive 1

Kai's Explosive 1 (frag, stick and conc grenades, the firebomb, the grenade
bag) imports beside Tier 1 with every behaviour ported (21/21 on the real
copy, sha256 1a850001…7b925a). Still unsupported: `RTB_registerPref` (16)
and `isFunction`, waiting for the settings seam on the Slayer branch.

Engine, each a generic seam:

- `Magazine::from_reserve`: Kai's `TT_grenade` items have no magazine; each
  throw takes `per_shot` from the reserve, nothing reloads. With none left
  the image leaves the hand while the tool stays selected; `set_reserve`
  mounts it again. The display shows the reserve alone (`AmmoView.counted`)
  and the light key works the light.
- `State::arm_once`: the molotov's `onArmed` played `spearReady` only while
  `getImageAmmo` was set, so a state looping into itself does not replay it.
- `Children::max_count` and `Children::steps`: `getRandom(a, b)` counts and
  per-axis `(getRandom(lo, hi) + offset) * step` velocities, mapped from
  v20 (x, y, z) to engine (x, up, back).
- `Aura::players_only`, `effect`, `target_sound` (new `Event::Heard`, a
  sound the sim sends that player alone) and `max_pulses`
  (`PrjLoop_maxTicks`).

Importer: `magazines.counted` names the item field marking counted
grenades; script-rule fills gain literal groups, `{=Text|field|ticks}` and
an `explosion` filter. A top-level `isFile` on a base-game Add-On (the
rocket launcher check) is no gap.

The stand-in fixture's embers carry no direct damage so the hosted test
reads each pulse; with direct damage an ember lying against a player hurts
them on every contact, as Torque's `onCollision` on each bounce.

Checks: `cargo test -p bri-addon-import --test tier_port` (6/6) and
`-p bri-weapons --test thrown_grenades` (5/5). The wider crate run and
clippy were cut short by the session's disk running out.
