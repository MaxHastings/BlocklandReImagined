# Bots: the Spear, the Gravity Gun's reach, and kinds with their own body

Max, playing v0.1.12: the bot used the Gravity Gun only right up close and
often backed off instead, and could not use weapons held back before they
fire, such as the Spear.

- Charged weapons: an image whose state lets go into its `onFire` state is
  charged (`Image::charges`, `Image::fires_on_release`, from its states, so
  every such weapon works with no data). A bot holds its trigger while it
  charges and lets go once letting go fires. Before, it tapped the trigger
  every 40 ticks, shorter than the Spear's charge, so it only ever aborted.
  Guard: `a_bot_with_a_spear_holds_it_back_then_throws_it` (bot_brain),
  fails on the old tapping.
- Gravity Gun: with no `reach` in its `bot` data the bot took it for a
  3-unit melee tool. Its image now says `reach` 30 and `near` 2.5 (new
  `BotUse::near`: the closest it is used from, so it grabs from close up
  instead of giving ground). Guard:
  `a_bot_with_the_gun_catches_from_across_the_room` (sim showcase), fails
  on the old data.
- Bot kinds (`bots.json`) gain, for the Zombie and Shark ports to come:
  `body` (an archetype, kept through respawns and mini-games), `melee` (a
  hit with its body when its hands are empty, `converts_below` for a
  zombie turning bots), `moves: swim`, `side` (Bot_Hole's `hType`),
  `look`, `emote` and `out_of_water_seconds`. Mini-game player types no
  longer replace a body an Add-On chose while alive, as respawns already
  did not. Tests in bot_brain and `bot_kind`.

Checked: cargo test bri-sim, bri-weapons, bri-chaos (729 passed), bri-net
(153), bri-addon-import (147); workspace clippy -D warnings clean.

Next: hole bricks (`Zombie Hole`, `Shark Hole`) spawning their bots, the
importer turning Bot_Hole bots' settings into kinds, and the Bot_Zombie and
Bot_Shark ports.

## Paused (weekly budget nearly spent, 2026-10-02)

Pick up here:
1. Zombie and Shark: hole bricks spawn bots by kind; the importer turns
   Bot_Hole bots' settings into `bots.json` kinds; then the Bot_Zombie and
   Bot_Shark ports. The Shark sets every swim speed to 0 in script, so its
   speeds must be chosen and said so; its mouth grab (hold, kill after 5 s)
   and colour variants (tinting an own-model body) can wait.
2. Bot redesign Max asked for (plan in the GG thread): scored behaviours
   instead of fixed priority, weapon handling read from weapon data, a
   richer walk map, shared battlefield awareness. Build in steps after 1.
