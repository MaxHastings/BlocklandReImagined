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

## Tier 1's click and its optional sound pack

Tier 1 requires Sound_Blockland only inside `if(isFile("Add-Ons/Sound_Blockland/server.cs"))`
and otherwise defines `Block_MoveBrick_Sound` and two more clicks from
`base/data/sound/clickMove.wav` and its neighbours. `Block_MoveBrick_Sound`
is in neither v20's scripts nor our audio packs, so on a v20-style
install the else branch is what runs.

- A require inside an `isFile` check for that same Add-On (in its block,
  or as its one statement) is dependency status `if_present`, not
  `missing`: the check reads as absent, as the `isFile` note says.
- `Reference::base_sound`: the base game's sound playing a file, from core
  scripts' filenames or the installed audio pack's clip ids. An Add-On's
  profile of a base file is `consumed` ("plays the base game's
  clickMoveSound"), and ports' `{s|sound}` fills name that base sound
  (`Code::sounds`), so the reload clicks play.
- A base-game `isFile` check is now reached before the general `isFile`
  note (the Adventure lane's branch had shadowed it).
- `file version.txt` is already `skipped` by the Adventure lane's rule (no
  script names it).

Checks: `cargo check -p bri-addon-import --tests` is clean. The tests
(`tier1_click_is_the_base_games_when_no_sound_pack_is_there` and the
`required_if_present` unit test) have not run yet: there is no disk space
to link them.
