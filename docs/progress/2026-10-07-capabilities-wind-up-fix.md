# 2026-10-07 Bots finish their wind-ups (the Butterfly Knife loop)

Branch `claude/project-thread-uoh3ap`, on the grenades head. Max's preview
playtest: bots with the Butterfly Knife "just go in a loop".

## Cause

Two things put a bot's wind-up away (`abort_bot_hand_charge`, which
remounts the image: Charge, then Activate, then Ready, then Charge again),
both in the hand trigger block of `session/bots.rs`, for weapons the native
chooser does not model (the knife's image has ported scripts, so it is read
by its states):

- In a fight, any one tick the bot was not on target (the enemy stepped off
  aim or out of reach) cancelled the charge.
- An idle goof click with a wind-up weapon pressed the trigger, and the
  same rule cancelled the charge the next tick, on every click.

The native path already kept a charge through such a tick while its
intent's target and weapon stood. The scripted path had no such rule.

## Fix

- One rule for both paths, `charged_control::keeps_wind_up`: a held,
  release-only wind-up stays held through a tick off target while the
  attack it was for still stands. The native path keeps its intent as
  that condition; the scripted path keeps the bot's enemy (in sight, or
  remembered) and the kind's own hold time since it was last on target
  (`behaviour::paused_hold`, `BotKind::hold_seconds`). Losing the enemy,
  that hold running out, the objective tool and a weapon switch still put
  it away. No new number.
- A goof's click owns the trigger: it presses and lets go as a player
  fidgets (with a wind-up, the early release), and is never cancelled as a
  fight's wind-up would be.

## Evidence

- `crates/chaos/tests/bot_windup.rs`: a bot with a wind-up knife (made-up
  data with the Add-On's state shape, and the imported Butterfly Knife when
  `BRI_CONTENT` is set) fights a player sidestepping at three paces. Every
  wind-up it begins in the fight ends in a full strike within the weapon's
  own charge ticks plus the kind's hold time, unless one of them dies.
  Before the fix it fails on both (the charge put away mid-fight); after,
  both pass.
- `bri-chaos` `bot_tactics` on the fighting lane's latest head (`0fa3aca7`)
  with this fix: 15 of 15 pass, `replacing_the_charged_equipment_cancels_without_throwing`
  included. On this branch's base (fighting `b6280a24`) that test and one
  other fail with or without the fix, as reported earlier.
- `bri-sim` `session::bots` unit tests pass.

## Release note

Bots can finish a wind-up attack (the Butterfly Knife's charged strike)
against a moving enemy; before, they almost never did (since v0.2.5 or
earlier), and an idle click with one made them restart it over and over.
