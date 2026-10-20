> **Historical note. Written 2026-10-06 (the file's date in the cloud project folder, `bots/human-feel-2026-10-06.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# How the bots come across as human, 2026-10-06

This reads branch `claude/v0.2.5-s4s5bn` at tip **2d45224** ("Bots: go explore"). I only read the code; I built and ran nothing. Paths are under `crates/sim/src/session/` and the shipped tuning is `packages/blockhead_bot/assets/bots.json`. The shipped Blockhead runs at surprise strength 0.6, perception strength 1 and teamwork 0.6.

## The short version

There are three kinds of variation, and only the first is the one Max wants.

1. **Shaped by what the bot knows.** Reaction time, the size of its aim error, glances, boredom, "this isn't working, try something else", copying a teammate, exploring. These change because something happened.
2. **Slow personal drift.** Each bot leans a little toward or away from each option, and the lean wanders over about 90 seconds. This is random, but it is slow and kept inside a narrow band, so it reads as mood rather than noise. Only options that are nearly tied can trade places.
3. **Plain coin flips.** Which goof to do and its details, where a brick bot strolls next, how long a strafe leg lasts. These are not shaped by anything the bot sees. This is where the bots can still read as random.

The main path for decisions (which behaviour, which weapon, where to aim, which way round a chase) is type 1 or 2 going through the one chooser and hold rule. The idle and "extras" side still uses type 3, and part of it bypasses the chooser. That part is the foundation gap.

## 1. Aim and turning

**Turning.**
- **What drives it:** turning works like a person's. The bot speeds up into a big turn, eases out, and overshoots a little on a fast flick (`bots/perception.rs:489-508`). Its top speed comes from the kind's turn rate.
- **What shapes it:**
  - Toward someone it hasn't reacted to yet, especially from outside its view cone, it turns more slowly (`turn_scale`, `perception.rs:560`).
  - Hit by someone it can't see, it freezes its look until it reacts (`startled`, `perception.rs:510`, used at `bots.rs` in the aim section).
- **Randomness:** none. It is all physics-like. **Shaped.**

**Reaction delay.**
- **What drives it:** the kind's reaction time, scaled by how alert the bot was: fighting, ordinary, or relaxed/goofing. It is slower again if the target was outside its view cone (`perception.rs:284-311`, `perceive` at `:608`).
- **What shapes it:** the clock only starts once the target can be hurt.
- **Randomness:** ±30 % jitter on top (`JITTER`, `perception.rs:94`). **Shaped**, with a little noise.

**Aim error.**
- **Size:** set by what the bot knows (`bots.rs`, aim section, "size ="):
  - its kind's base error;
  - narrowing as it keeps tracking the same target, down to a third after about 2 s (`ERROR_FLOOR`);
  - wider just after a slow reaction (`aim_scale`, `perception.rs:476`);
  - a lasting extra error when the target moves across its view fast or far away (`steady_error`, `perception.rs:570`).
- **Direction:** a smooth seeded wobble that never jumps and never settles (`aim_error`, `bots.rs:800`, `cadence::drift`). This is noise, but smooth.
- **Misses are real.** The fire gate judges where the bot *believes* it aims, so a miss happens rather than a shot being held back (`believed`, `perception.rs:577`).
- **Lead:** about right, drifting ±15 % over time (`lead`, `bots.rs:812`). This is noise.
- **Where it aims:** torso, feet or nearby surface is a real choice through the chooser (`surprise.rs:170`). Splash weapons aim at the feet.
- **Verdict:** shaped size, smooth-noise direction. Good.

**Firing rhythm.**
- **What drives it:** each bot taps on its own beat (`cadence::beat`, `bots.rs`, `bot_act`). It fires in bursts with pauses on its own seeded cycle (`cadence::bursting`, `cadence.rs:82`).
- **Gap:** this is a **timer**. It doesn't respond to hits, misses or the target ducking out.

**Glances.**
- **What drives it:** an idle bot looks at blasts, loud sounds, fast-moving things, someone staring at it, or someone nearby. The chance comes from how big or loud or close the thing is, from engine data only (`perception.rs:1-30`, `bot_glance` at `:694`). Glances have a cooldown.
- **Randomness:** strolling, its look drifts a few degrees on two slow swings (`drift`, `perception.rs:593`). That is smooth noise. **Shaped.**

## 2. Emotes and goofing

**Whether to goof: a real choice.**
- Playing and goofing are two options through the chooser and hold rule (`surprise.rs:674-787`).
- **What shapes it:**
  - Playing gets boring while the bot plays, twice as fast with nothing to do (`PLAY_BOREDOM`, `IDLE_BOREDOM`).
  - A goof is worth most in a lull and little with the action close by (`LULL`, `ACTION_NEAR/FAR`).
  - Others visibly goofing make it likelier (`MOOD_LIFT`, from `team::mood`).
  - Doing a goof relieves the boredom.
- **When it can't goof:** never while carrying an objective or when urgent (`Gate`).
- **Verdict:** this is the good, human part.

**Which goof: a weighted coin flip.**
- After goofing wins, the flavour (stare, emote, hop, circle, detour, look, crouch, spray, tool, drop, light, door) is drawn by roulette from **fixed weights** (`surprise.rs:754-762`).
- **What filters it:** only whether the flavour is possible (`possible`, `:704-715`). "Copy" adds a bonus for goofs it saw others do.
- **What it ignores:** the situation. It does not stare more at someone staring at it, emote more after a kill, or hop with others who are hopping.
- **Details are coin flips too:**
  - which emote, which tool slot, which spray colour, detour angle and distance (`surprise.rs:1297, 1302, 1324, 1349-1350`);
  - walk up close or not, 50/50 (`:774`);
  - gait (`:778`);
  - length, 2-3 s (`:766`).
- **Bypasses the hold rule:** it is held by its own timer, not the hold rule.

**Emotes on purpose.**
- The kind's own emote plays once per life (zombie arms, `bot_kind_rules`, `bots.rs`).
- There is no emote in response to an event (a kill, a death, a goal).

## 3. How they move around

**Route and stance.**
- The route comes from the planner. Stance in a fight is general:
  - melee: lunge, back out, step aside, plant, on its own seeded cycle (`footwork`, `bots.rs:839`);
  - ranged: strafe legs, turning at walls, ledges and allies, with a slow lean in (`bots.rs`, stance section).
- All of it now goes through one act stage with safety last (`bots/act.rs`).
- **Gap: strafe legs are coin flips.** Their length is mean × 0.5-1.5 and they turn back 70 % of the time (`strafe_leg` and `leg`, `bots.rs:555-579`, `:818`). They don't react to whether the enemy is tracking it.

**Dodge (new today).**
- A dodge is a real choice between hop, strafe and jet within its fuel (`extras.rs`).
- **Bypasses the hold rule:** it always interrupts, so commitment comes from private timers (`crouch_until`, `next_dodge`).

**Strolling.**
- **Brick bots:** pick a **random point** within 12 units of their brick every 2 to 6 s (`wander`, `bots.rs:662-685`). That is a pure coin flip on a timer.
- **Rules bots on big maps (new today):** choose by exploring (`bots/explore.rs`). The scorer prefers places not visited lately, near where enemies were last known, and away from where teammates are going, with a little seeded variety. **Shaped.**
  - The commit reports wander dropping from 79-89 % to 4-6 % on Max's A TDM and Afghanistan saves.
  - Stuck time on Afghanistan rose from 6 % to 23 %, because bots now reach the map's stuck spots. **That needs fixing before ship.**

## 4. Boredom, and picking a less-than-best option

Every choice the chooser handles (behaviour, weapon, aim point, flank route, goof-or-play, move, dodge) scales each option by three terms before the hold rule decides (`surprise.rs:485-579`).

**Boredom.**
- **How it works:** an option grows boring while in use and fades with a 20 s half-life once dropped (`BOREDOM`, `BOREDOM_SECONDS`, `accrue` at `:581`).
- **Shaped:** by what the bot has actually been doing.

**Effectiveness.**
- **How it works:** an option that fails loses value, and one that works gets it back:
  - fails: a shot dodged, no damage done, stuck;
  - works: a hit, progress made.
- **Fade:** a 30 s half-life (`outcome`, `:592`; called from the stuck judge and from shot results).
- **Shaped:** by results. This is the "this isn't working, try the other way" behaviour.

**Drift.**
- **How it works:** a slow random walk per bot and per option, crossing its range in about 90 s (`drive`, `:438-482`, `DRIFT`). Each bot starts at a different lean.
- **Verdict:** random, but slow. It reads as preference or mood, not twitch.

**Copy.**
- **How it works:** a bonus for an option the bot saw work for a teammate (`saw`/`seen`, `:621-655`).
- **Shaped:** by what it saw.

**Limits.**
- Drift and boredom together move a score at most ±15 % (`BAND`), so only near-tied options ever trade. The bot never picks something silly.
- Nothing varies while it carries an objective or is urgent: low health, or hit from close range just now (`Gate`, `surprise.rs:203`; `URGENT_*`). Then it plays its plain best.

**Is this human?**
- Mostly, yes. It is sub-optimal in the right way: small, explainable, and it changes after failure.
- What it does not do is make **pressure mistakes**. Real players get sloppier when hurt or rushed, and these bots get *more* exact when urgent. That is a design choice, not a bug, and it suits fairness. It is a possible later addition.

## Where it falls short

### Core design gaps (Max's foundation-first rule says v0.2.5)

These matter because the design promises one score per option and one chooser with the hold rule. These paths skip that.

1. **Which goof is a fixed-weight roulette, with coin-flip details** (`surprise.rs:754-778`). Each flavour should be its own option, scored from what the bot perceives, through `Mind::pick` and the hold rule:
   - a stare scores higher when someone looks at it or stands near;
   - an emote scores higher just after a kill or death;
   - copying scores higher when others nearby do the same goof.

   **Medium.** This is the biggest "reads as random" gap.
2. **Move/dodge, weapon hand-offs and the route door click run on private timers and beats, not the hold rule.**
   - Move and dodge always pass `interrupt: true` (`extras.rs:408`), and `crouch_until` short-circuits the choice.
   - Weapon hand-offs and the route door click run on cadence beats with rest timers (`extras.rs:521-541`, `670-747`).

   **Medium.** This is the same fix pattern as item 1.
3. **Brick bots' stroll is a random point on a 2-6 s timer** (`bots.rs:662-685`). Explore already has a scorer for "where is worth walking to". Reusing it for strolling, inside the brick's radius, gives one mechanism instead of two. **Cheap.**

### Not foundation, but before ship

4. **Explore's stuck time on Afghanistan, 6 % → 23 %.** These are the route's existing stuck spots, now reached. This is a real gameplay regression from today's commit, as reported in its own message.

### New behaviour for later

- **Strafe legs that react** to being tracked or shot at, instead of a 70 % coin flip (`bots.rs:574`).
- **Burst rhythm that reacts** to hits, misses and the target hiding, instead of a fixed cycle (`cadence.rs:82`).
- **Event emotes and taunts,** such as after a kill, a death or a goal, scored like any goof.
- **Pressure sloppiness,** a little worse aim or choices when hurt or rushed, if Max wants it.
- **More than one callout line per term.** Today it is one fixed line ("Coming!", "Moving!", "I got him, go!", `bots.json:22`), which repeats verbatim. This was row 20 of the robotic-tells list.

## The robotic-tells list (2026-10-05), checked against today

- **Fixed:**
  - #1 clone avatars: seeded look per bot (`lifecycle.rs:172-183`).
  - #2 statues when idle: surprise is on at 0.6, with glances.
  - #3 ignoring nearby players: a strolling bot glances at players near it.
  - #4 unison fire and #11 unison dismount: per-bot beats.
  - #5 aimbot rockets: smooth error, feet aim, lead stray.
  - #6 squad snap: a delayed, offset warning (`perception.rs:520-545`).
  - #7 machine cadence: bursts.
  - #12 lockstep respawn: a seeded wait.
  - #13 plant-and-chop: footwork.
  - #14 no dodging: the dodge.
  - #16 same reaction: alertness and jitter.
  - #18 rhythmic stuck-hop: one hop per stall window.
- **Fixed, probably:** #8 robot names. Brick bots draw from `first_names` (`lifecycle.rs:411-425`). I didn't trace every path.
- **Partly:** #17 eyes in the back of the head. There is only a slower reaction from outside the view cone, with no rear blind spot.
- **Still open:** #20 one callout line per term.
- **Not checked today:** #10 vehicle stop-start throttle, #15 ball-locked eyes and #19 the same life replayed after a brick respawn.
