> **Historical note. Written 2026-09-30 (the file's date in the cloud project folder, `bots/bot-ideas.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Blockhead Bots: where they are and where they could go

Checked against main 8a20841 (`crates/sim/src/session/bots.rs`, 331 lines).
v20 facts below are checked against the decompiled scripts.

## What our bots do today

- Spawn from a Vehicle Spawn brick set to "Blockhead Bot". Up to 16 per server.
  Each is a normal server player with no connection, so health, damage,
  inventory, minigame rules and replication all apply unchanged.
- Idle: wander to a random point within 12 studs of their brick every 2 to 6 s.
- Combat: only inside the brick owner's minigame. Targets the nearest human it
  can see within 80 studs (one ray per candidate per tick), snaps its aim onto
  them instantly, holds 5 to 14 studs away, strafes left/right every 0.75 s,
  jets if the target is above, fires when facing, re-pulses the trigger.
- Equips the first real weapon in the minigame's starting inventory.
- Stuck: hops after ~0.2 s of no progress, gives up on the goal after ~1.7 s.
- Dies and respawns at its brick after ~1 s. `/clearbots` and reset vehicles work.
- Rideable: the rider in seat 1 steers it (horse-style). `onBotTouch` events fire
  with the Bot and Driver targets, run as the brick owner (v20's rule).

### Where that feels oversimplified

1. **No path finding.** Straight-line steering plus hopping. Walls, stairs with
   gaps, and anything taller than a jump trap them (KNOWN-ISSUES says so).
2. **Perfect, instant aim.** Yaw snaps onto the target's eye every tick. Hard to
   dodge, and it looks robotic.
3. **Blind to being hurt.** Shot from outside its view or from behind a wall, it
   keeps wandering. It never investigates a sound or a hit.
4. **One distance for every weapon.** A sword bot stands 5 to 14 studs away and
   swings at air; a rocket bot walks to 5 studs and blows itself up.
5. **Ignores other bots.** Target list is humans only, so bots never fight each
   other in team minigames (red bots vs blue bots is impossible).
6. **No health sense.** Never retreats, never picks up items or health.
7. **No builder control.** A builder can place it and wire `onBotTouch`, and
   that's it: no way to say "guard here", "follow me", "walk to that brick".
8. **One look.** Always the default Blockhead, name "Blockhead Bot".

## Group A: what v20 actually did (checked in the decompiled scripts)

Source: raw grep of the v20 decompiled scripts on the PC (kept off the repo,
in /mnt/project-files/v20-ai-grep.txt).

- A Vehicle Spawn brick makes an `AIPlayer` for any player-type datablock
  (`fxDTSBrick::spawnVehicle`, allGameScripts 16844-16964). The picker lists
  only player types that have a UI name **and are rideable** (client 15786-15807,
  server 2649-2679). So stock v20's "bots" are rideable animals and mounts like
  the Horse, never a plain Blockhead.
- Stock v20 gives them no brain: nothing calls setMoveDestination,
  setAimObject or setMoveObject. The engine callbacks onReachDestination,
  onTargetEnterLOS and onTargetExitLOS are defined on Armor (10026-10034) but
  nothing drives them.
- What v20 does have: `onBotTouch` (Bot, Driver, Client, MiniGame targets,
  17136), riding, spawn/respawn/recover through the brick, minigame vehicle
  respawn, and `/clearbots` (admin only; deletes every unmounted player object
  nobody controls and messages "cleared all bots (N)").
- The v21 bot kit (Bot Holes, zombies, bot events) is not in v20.

What that means for us:
- Our wandering, fighting "Blockhead Bot" is already beyond v20. Under the
  product rule (extras ship as opt-in Add-Ons), it belongs in an Add-On,
  installed but off, like Ragdoll. The base game keeps v20's brainless
  rideable mounts, onBotTouch and /clearbots, which we already have.
- Fidelity gap found: v20's /clearbots deletes every spawned player-type
  mount (Horse and other rideable mounts) as well as bots. Ours
  (`admin_clear_bots`, crates/sim/src/session/admin_world.rs:199) clears only
  Blockhead Bots and counts only those.

## Group B: beyond v20 (engine mechanism in the base game, policy in Add-Ons)

1. **Smarter base brain** (engine, no protocol change, no bandwidth):
   human aim (turn rate plus a short reaction delay and small spread),
   weapon-aware ranges from the weapon's own data (melee closes in, rockets
   keep distance), turn toward whoever hurt it, remember the last seen spot
   and go look, fight other bots on other teams.
2. **Path finding on the brick world.** A walk grid built lazily around each
   bot's home from brick tops and map collision, cached per chunk, rebuilt only
   when bricks in that chunk change. A* on it, capped nodes per tick, shared by
   all bots. Cost stays near zero on a 1M-brick city because only chunks near a
   bot are ever built.
3. **Bot events (the v21 idea, as a generic hook).** Inputs: onBotSpawn,
   onBotDeath, onBotSeePlayer, onBotDamaged, onBotReachGoal. Outputs on the Bot
   target: SetGoalBrick / WalkTo, Follow(player), Guard(radius), Attack/
   Ignore, SetWeapon, SetAppearance, SetName, SetSpeed, Say. Builders get
   guard posts, shopkeepers, escort missions, tower defence, all with the
   wrench, no scripts.
4. **Behaviour profiles as Add-Ons.** Zombie (melee, swarms, no guns),
   Guard, Follower/companion, Wanderer/townsperson, Shark/animal. Each is an
   Add-On built on the same hooks, so modders can make their own.
5. **Difficulty and cap per minigame.** Easy/Normal/Hard sets reaction time,
   aim spread and sight range. Bot cap stays a host setting (16 today).
6. **Appearance.** Bot spawn brick picks colours, face and decal (reuse the
   avatar data); rides the existing player replication, no new traffic.
7. **Later:** bots driving vehicles, bots building (probably not; low value).

## Cost rules they all obey

- Network: bots are players, so everything above reuses existing pose and
  appearance traffic on UDP 28000. Nothing cosmetic gets new messages.
- Performance: brains run only for existing bots, path finding is budgeted per
  tick, grid chunks build only near bots. Zero cost on a world with no bots, so
  the 1M-brick headline is untouched.
- Content: nothing from v20 or Add-Ons is committed; profiles are our own.

## Recommendation

0. **Move the Blockhead Bot into an opt-in Add-On** (installed but off), since
   v20 never had it. Cheap, and it keeps the base game faithful.
1. **Smarter brain + path finding first.** They fix the everyday feel
   (stuck bots, robotic aim, sword bots at range) for every bot, touch only
   `bots.rs` plus a new nav module, and need no new UI or protocol.
2. **Bot events next**, as the generic hook. That's where the fun is for
   builders and it's what modders will build on.
3. **Profiles and appearance** as opt-in Add-Ons after that, Zombie first
   because it's the most fun per line of code.
