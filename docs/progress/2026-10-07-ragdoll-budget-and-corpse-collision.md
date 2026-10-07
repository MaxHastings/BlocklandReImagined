# 2026-10-07 Ragdolls that stop, and corpses that block players

Max, v0.2.5: ragdolls sometimes go through the floor, or stop completely
(the Ragdoll on, the body falling back to the death animation); and a dead
body blocks living players, which is annoying in minigames.

## Corpses block living players

Cause: a death never changed the body's collision. `Player::set_solid(false)`
ran only at the five-second corpse timeout, so for five seconds the corpse's
box stood in the way of every living player's motor. The client's prediction
(`motion.rs`) already left corpses out, assuming the host did too, so a
player walking into one was also corrected back.

v20: `Armor::onDisabled` (decompiled `allGameScripts-Vanilla.cs`) changes no
collision, and the engine's movement code is not in the repository, so the
engine side is unconfirmed. Max's recollection and the brief's default
decide it: dead bodies do not block living players, but stay solid to
weapons, vehicles and grabs (the Gravity Gun still picks corpses up).

Fix: `Player::set_corpse` puts a dead body's collider in a `CORPSE`
collision group at death (both death paths) and takes it out at respawn.
Every player motor query (`step`, `Player::clear` for spawns) uses
`MOVES_AGAINST`, which skips that group; nothing else filters groups, so
weapons, vehicles, grabs and the timeout's sensor are unchanged.

Evidence: `cargo test -p bri-sim --test session living_players_walk_through_a_fresh_corpse`
failed before the fix (blocked at x 0.75, the corpse at 2) and passes after.

## Ragdolls stop

The game stops an Add-On whose physics takes over 4 ms a frame for 30 frames
in a row (`Budgets::physics_ms_per_frame`, `physics_strikes`), for the rest
of the session: every later death plays the v20 animation. A probe of the
real Ragdoll Add-On (release build, Max's PC under other lanes' builds)
found two causes:

1. Any build change anywhere (a brick planted or killed, a door, a light
   blinking with `setRendering`) bumped `Building::query_generation`, and
   `Surroundings::sync` then dropped every brick round every body and woke
   them all. Bodies never rested and reloaded hundreds of colliders: 8
   ragdolls with a far brick changing every 7 frames averaged 5.4 ms, every
   frame. Now `sync` drops only the loaded bricks that changed or went
   (waking bodies only then), and a brick new to a resting body's
   surroundings wakes that body alone. Same case after: 2.3 ms mean, settling
   to 0.04 ms as ragdolls rest. Brick debris shares the change and still
   wakes on a build change, without the rebuild.
2. Ragdoll joints were a multibody, about 1 ms a frame per tumbling
   Blockhead: six deaths a third of a second apart ran 61 frames over 4 ms,
   fourteen 280 frames. Impulse joints with four joint solver passes
   (`JOINT_PASSES`) cost a fifth of that: fourteen ragdolls 1.06 ms mean,
   2.3 ms worst, never over 4 ms. The multibody was chosen on 2026-09-30
   because impulse joints stretched 0.19 on a rocket landing; that was with
   swept CCD, since replaced by soft CCD. Now the blast test's worst stretch
   is 0.004 (0.027 with one pass) against its 0.05 limit, and the content
   checks on the real Blockhead pass. A hold carries everything jointed to
   the held body (`carried_mass` walks the joint graph).

## Through the floor

Not reproduced. The probe dropped ragdolls from up to 150 units onto brick
decks, 0.2 plates and one-layer map floors, killed them at 50 u/s into the
floor, walked a player box over them, and churned the build: no body passed
through a floor; the only drops were limbs knocked off a deck's edge. The
reported fall-back to the death animation matches the budget stop above. If
it still happens after this, a save and where it happened would let the
probe replay it.

## Commands

- `cargo test -p bri-client --lib` (486 passed), with
  `BRI_CONTENT=<main>/content cargo test -p bri-client --lib ragdoll -- --include-ignored`
  (8 passed, the real-Blockhead ragdoll checks included).
- `cargo test -p bri-motor -p bri-sim --lib --test session --test combat --test showcase --test prediction --test player --test vehicles --test hardening_session`.
- New: `a_build_change_elsewhere_leaves_resting_bodies_alone`,
  `a_brick_built_into_a_resting_body_wakes_it`,
  `a_hold_lifts_everything_jointed_to_the_body_it_holds`,
  `living_players_walk_through_a_fresh_corpse`.
