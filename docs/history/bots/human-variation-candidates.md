> **Historical note. Written 2026-10-06 (the file's date in the cloud project folder, `bots/human-variation-candidates.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Human variation from what bots already know: candidates, 2026-10-06

This reads branch `claude/v0.2.5-s4s5bn` at tip **2d45224**. I only read the code; I built nothing and ran nothing. Paths are under `crates/sim/src/session/`.

Rule for this list: each candidate reuses a signal the bot already has (sight, hurt, health, alertness, boredom, effectiveness, the perception watcher, team posts) and an existing mechanism. I dropped anything that needs new state, a new system or a per-case rule, and I don't count dice.

Sizes:
- **Tiny** is a few lines in one place.
- **Small** is one function and a test.
- **Medium** touches a few places.

## Ranked by value to a player for the effort

### 1. Searching, it looks around like a person, not a turret (do in v0.2.5)

- **What the player notices:** today a bot that reaches the spot where it lost you spins at a constant rate (`aim_yaw = brain.yaw + 0.8 * TICK * 2.0`, `bots.rs:2678-2679`). It reads clearly as a machine. Instead it would look one way, then back the other, with uneven pauses, and favour the direction you were last seen heading.
- **What drives it:**
  - the smooth seeded signal `cadence::drift` (`cadence.rs:68`), which the aim error and idle look already use;
  - the remembered evidence `brain.memory` (`bots.rs`, step_bot).
- **Size:** tiny.
- **New state:** none.

### 2. Hit while strafing, it turns the other way (do in v0.2.5)

- **What the player notices:** today a strafe leg runs its seeded length and turns back on a 70 % coin flip (`strafe_leg`, `bots.rs:560-584`; `TURN_BACK`). After this change, a bot you hit mid-strafe switches direction, so leading it stops working twice in a row.
- **What drives it:**
  - `hurt_by` / `hurt_now` are already computed this tick (`bots.rs:1891`);
  - `strafe_leg` already takes a `parted` flag that forces a turn (`bots.rs:2837-2838`).

  Passing "hurt this tick" in alongside it is the whole change.
- **Size:** tiny.
- **New state:** none.

### 3. It stares back at someone staring at it (do in v0.2.5)

- **What the player notices:** stand and look at an idle bot, and it notices and looks back, or does a stare goof at you. Today the goof's target is just the nearest visible player within 24 (`surprise_pause`, `surprise.rs:1217-1229`).
- **What drives it:** perception already tracks who is watching the bot and since when (`State.watcher`, `perception.rs:343, 402-413`; found by `bot_watcher`). Two changes:
  - prefer the watcher as `Pause.player`;
  - scale the Stare flavour's weight up while someone watches.
- **Size:** small.
- **New state:** none. The `watcher` field exists.
- **Side benefit:** it replaces that 24-unit, all-round sight check with perception's own.

### 4. A lazy head when relaxed, a snappy one in a fight (do in v0.2.5)

- **What the player notices:** a strolling or goofing bot turns its head unhurriedly. One that is fighting or hunting turns at full speed. Today every turn uses the same top speed, whatever the bot is doing.
- **What drives it:**
  - alertness (`Brain::alertness`, `perception.rs:662`: Combat, Ordinary or Relaxed, with goofing counting as Relaxed);
  - the kind's `relaxed_scale`, which already slows reactions.

  Multiply the turn step by the same scale while Relaxed (`step` in the aim section of `step_bot`).
- **Size:** tiny.
- **New state:** none.

### 5. A bored bot fidgets more

- **What the player notices:** the longer a bot has idled, the more its look wanders, until it goofs. A fresh bot looks where it walks.
- **What drives it:**
  - boredom of playing (the Flavour/PLAY drive, `surprise.rs:581`);
  - the idle look drift (`perception::drift`, `perception.rs:593`), scaled by `1 + boredom`.

  This needs a read-only getter on the drive.
- **Size:** tiny.
- **New state:** none.

### 6. Hurt, it gives more ground

- **What the player notices:** a low-health bot keeps a bit more distance: a ranged bot backs off, and a melee bot steps out more between swings. It doesn't trade at point blank like a full-health one.
- **What drives it:**
  - the health share already read for the urgent gate (`surprise.rs:951-954`);
  - the band's near edge `near`, which drives `back_off` and footwork.

  Widen `near` as health drops.
- **Size:** tiny.
- **New state:** none.
- **Watch:** it shifts fight balance, so judge it on a quick match before keeping it.

### 7. Social goofs near people, solo goofs alone

- **What the player notices:** with a player close by, a bot's goofs lean toward staring, emoting or walking up. Alone, it leans toward a hop, a crouch, a circle or a look round.
- **What drives it:** `Pause.player` and its distance, already computed. Scale the existing flavour weights by it (`surprise.rs:754-762`).
- **Size:** small.
- **New state:** none.
- **Fits with:** the foundation fix I recommended earlier (each goof becomes a scored option). Do it as part of that.

### 8. Missing, it fires in shorter bursts and re-settles

- **What the player notices:** a bot that keeps missing pauses a little longer between bursts. One that is landing hits keeps pressing.
- **What drives it:**
  - the weapon's effectiveness, already updated from shot outcomes (`Mind::outcome`, `surprise.rs:592`; due shots `:666`, judged at `:1064-1080`);
  - the burst-pause share in `cadence::bursting` (`cadence.rs:82`), which would take it as a factor.
- **Size:** small.
- **New state:** none.

### 9. Shakier aim when badly hurt

- **What the player notices:** a bot on low health, or one just hit from close by, aims a little worse for a moment.
- **What drives it:** the same health and `hurt_now` signals. Multiply the aim-error `size` (step_bot aim section) behind the kind's perception strength.
- **Size:** tiny.
- **New state:** none.
- **Watch:** this is a difficulty change, and Max's "fair" test applies. It's optional.

### 10. Brick bots stroll somewhere worth looking at

- **What the player notices:** brick bots stop picking a random spot every few seconds (`wander`, `bots.rs:662-685`). They walk to places they haven't looked at lately or where they last saw someone, inside their usual radius.
- **What drives it:** the new explore scorer and its per-brain memory (`bots/explore.rs`, `Brain.explore`), bounded by the brick's `wander_radius`.
- **Size:** small to medium.
- **New state:** none. Explore's state already exists on every brain.
- **Fits with:** the earlier "reuse explore for strolling" item.

## Dropped (would need new state, data-shape changes or a special case)

- **Emote or taunt right after a kill or a death.** The bot only knows its target died for one tick (`target_gone`). Making it last needs a small "just won" memory, which is new state. Goofs already become likelier in the lull after a fight, because playing is worth less with nobody around (`LULL`).
- **More than one callout line per term.** This changes the `bots.json` shape. It's not context.
- **A rear blind spot.** This changes what sight means. It's a perception design change, not a tweak.

## What I'd do in v0.2.5

Items **1 to 4**. They're each tiny or small, clearly visible on camera, and they don't change difficulty or balance.

Fold item 7 into the goof-chooser fix if that goes in. Items 6 and 9 change balance, so try them on a quick match first. Items 8 and 10 are next version unless there's room.
