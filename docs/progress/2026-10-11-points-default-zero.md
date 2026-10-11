# 2026-10-11 Points default to zero except a kill

Max (v0.2.10): setting up his Slayer soccer meant hunting through the
Mini-Game window and Slayer's Points and Bonus Kills pages to turn off every
point source, and teams still gained points without goals.

## Decision

Generic point sources are 0 by default and a kill is +1; a mode's own
objective keeps its natural value. A ball or coin game needs one change:
Kill Player to 0.

- Kill Self 0 (v20's form had -1), Die, Plant Brick, Break Brick 0.
- Slayer: Friendly Fire 0 (was -3), Bonus Kills off.
- Slayer: new Bots Worth Kill Player (on): Slayer's bots are worth Kill
  Player; off shows Slayer's Kill Bot again.
- A Bonus Kills bonus only adds to a kill worth more than 0: with Kill
  Player 0 the kill feed still calls sprees but nobody scores.
- Bonus Kills moved onto Slayer's Points page; its numbers show only while
  it is on.
- Kept: CP Capture, CTF flag points, a goal's IncScore. They fire only where
  a builder placed the objective, and a CP or CTF map is meaningless at 0.

## What existing setups do

No migration (alpha rule):
- Add-On favourites store every value, so they keep their numbers, except
  that a favourite with Kill Bot different from Kill Player now scores bots
  like players until Bots Worth Kill Player is turned off.
- Saved builds store Add-On settings apart from their defaults: one that
  relied on Bonus Kills on, Friendly Fire -3 or Kill Self -1 loads with the
  new defaults.

## Evidence

- `cargo test -p bri-addon-import --test slayer` (45 passed), including the
  new `kill_player_at_zero_makes_every_kill_worth_nothing`; with the bonus
  gate removed that test fails (score +5 from a spree at Kill Player 0).
- `cargo test -p bri-minigames -p bri-ui -p bri-package-runtime` pass.

## Tools

`soccer_probe` now traces every player and team score change with that
tick's deaths (and killers), chat lines, vehicle jumps and round results,
to find what scores in a saved ball game with an Add-On favourite.

## What scored without goals in Max's soccer

`soccer_probe` on Max's PC, save "Soccer Field Goo" (Skylands) with Add-On
favourite 1 (Slayer Team Deathmatch, Points 3), 300 s, 8 bots:

- The favourite has Slayer's Kill Bot 0 but the Mini-Game window's Kill
  Player 1 and Kill Self -1. Kill Bot only replaces Kill Player when a
  Slayer bot dies, so every kill of a human (the host) still paid the
  killer's team a point, and every suicide or fall cost one. With Points 3
  to win, bots killing Max three times won a round with no goal. Bot kills
  showed +1 then -1 a tick later (Slayer's correction); Slayer's own
  victory check runs after the correction, so that flicker never won.
- The three team points with no death attached were real goals: a goal's
  resetObject brings a new ball, which the probe counted as no reset.
  The probe now counts a replaced ball and prints new vehicles, and its
  final team scores include the team's own points.

With this change, Kill Player 0 alone stops every kill from scoring.
