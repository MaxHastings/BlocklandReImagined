> **Historical note. Written 2026-10-08 (the file's date in the cloud project folder, `v027/bot-ownership.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Bot brain ownership map (main d6161ef, 2026-10-08)

Every bot tick in `step_bot` (crates/sim/src/session/bots.rs) runs these stages: sense, choose a behaviour, set a goal, route, propose, then `act::resolve`, then safety. The controls themselves already have one writer, `act::resolve` (bots/act.rs:236). Several things upstream can still decide *where the bot should be* and *which way to step*, so the proposals that reach resolve can disagree. This map lists who decides what, and where it meets resolve.

## Who decides what

| Responsibility | Decides today | Feeds resolve as | Overlap |
|---|---|---|---|
| Behaviour (fight, objective, search, wander, interact, arm, goof) | chooser in `behaviour.rs` (0.5 s hold rule) | which goal and stance below | Goofs pick play vs goof on their own (`surprise.rs` ~588). Idle play is an Interact score (`interactions.rs:20`), and Wander is a third path. Extras (crouch, dodge, door, hand weapon) roll their own `extras.rs chance()` |
| Where to stand (goal) | 27 `set_goal` calls: 20 in bots.rs, plus extras.rs 651/740/744, surprise.rs 1409/1505, interactions.rs 709, lifecycle.rs 371 | `Mover::Route` via the plan | Surprise and extras overwrite the chooser's goal directly, not as a behaviour |
| Ranged fight position | `spots.rs` `bot_stand` (here/left/right/in/out/hop/jet), as a `Goal::Stand` route | `Mover::Route` | **The left/right spot and the strafe both move sideways** (two owners) |
| Fight footwork | stance block (bots.rs ~3080-3156): weave, melee footwork, ranged strafe | `Mover::Stance` | Strafe runs only once the route is done. Route and strafe alternate, which shows up as route/stance flicker |
| Objective movement | the objective goal (`Goal::Objective` route) plus `pushing` (an interaction's push) | `Mover::Route` and `Mover::Push` | Two carry controllers: the old grab-and-fling Carry and the objective body hold, guarded by `objective_hold_control` |
| Route legs | route.rs jet and leap legs | `Mover::Jet` / `Mover::Leap` | none |
| Swim | bots.rs ~3239-3260 | `Mover::Swim` | none |
| Dodge | dodge (`bot_incoming`) | `Mover::Dodge` | `bot_live_blasts` duplicates `bot_incoming`'s blast read, and the dodge has its own flight model |
| Stand still | hop, hand-off, goof that stands | `Mover::Stand` | none |
| Unstick | `route::Progress` stalled, then hop (bots.rs:3327), then replan (3359) | press on top of resolve | **Three stuck detectors** (progress.stalled, stuck, wedged, ~3676-3697) and **two hop-when-stuck paths**. The trapped clock (`trapped`, :224) leads to Respawn and counts unreachable stroll goals |
| Look | `act::look` over `Looker` (hold, sweep, route, glance, target, objective, carry, down, gesture) | one look | none, already one owner |
| Aim and turn | bots.rs 3029-3036 (yaw and pitch step) after the look | `input.yaw`/`pitch` | none |
| Trigger | bots.rs 1475-1479 (goof, objective, fight) | `act::Trigger` | Two ally-shot readers |
| Seated or driving | interactions.rs 1420-1677, which writes `input` directly | bypasses resolve (a seat is its own controller) | Ally corridor braking checks only allies' current feet, not where they are moving (jeep teamkills) |
| Safety | `bot_safe_walk` after resolve, which only takes away | last word | none |

## Consolidation, in order (small steps, each measured)

1. **One sideways mover in a ranged fight.** Spots choose *where* to fight from: here, in, out, hop or jet. The stance owns sideways footwork, so left/right becomes the strafe's side choice rather than a route. The parked branch tried this and grenades broke, so the strafe must cover what the left/right spot was for: stepping out of cover for a clear throw. Planned shape: the strafe takes the spot's side and holds it until the shot is clear, rather than weaving.
2. **One unstick owner.** `route::Progress` judges stalls. Hop, replan, give-up and trapped are its steps, in one escalation. The other two detectors and the second hop path go.
3. **Goals only from behaviours.** Surprise and extras propose a behaviour (goof or wander) to the chooser instead of calling `set_goal` themselves.
4. Later, not v0.2.7: one carry controller, one blast reader, and one idle-fun picker.

Evidence for each step: the gauntlet, `team_fill_bots_play_slayer_soccer_with_or_without_weapons`, `bots_play_an_unfamiliar_package_by_its_own_rules`, and bot_watch matches on Soccer, Close Quarters, ACM City and Afghanistan. The clunk counter measures walk and look hand-overs, reversals and flips.

## What depends on each competing path (read from code before changing it)

### Sideways in a ranged fight: `spots` left/right vs the stance strafe
- **Left/right spot** (`spots.rs` `bot_places`, `bot_stand`). This is how a ranged fighter steps a body's width aside when its shot from here is blocked or keeps missing. Grenade and rocket throwers use it to clear a wall or ledge before throwing. Each place is scored by the shot chooser run from there (`spots.rs` 11-21), so it is shot-seeking. While a spot is set, `pursue` routes to it (`Goal::Stand`), and the strafe is off (`wanted.is_none()`).
- **Strafe** (bots.rs stance block). This is evasive footwork once the bot stands where it fights. It turns at edges, walls and allies and when hit (`strafe_leg`). It knows nothing about the shot.
- **The conflict.** On arrival the spot clears (`there`) and the strafe resumes, often in the other direction, back behind the cover the spot just stepped out of. On the next planning turn the spot is weighed again, and the bot steps out again. This is the route/stance flicker.
- **Why the parked attempt broke grenades.** It turned left/right into a strafe side and dropped the route. The strafe walks at 0.7 speed, stops at allies and turns when hit, so a thrower never reliably cleared its cover. It threw into the wall next to its own team.
- **Single-owner rule.** Spots own *where* (including a step aside). The strafe owns *how it moves once there*. When a left/right spot is reached, the strafe's current leg takes that spot's side, so it never walks straight back into the cover it stepped out of. The route still walks the step-aside at full speed, as main does.
- **Unaffected.** Melee footwork never uses spots (ranged only, bots.rs `!weapon.melee`). Swimmers and objective weaving also don't use spots.

### Objective movement (soccer)
- Unarmed soccer bots (Slayer team fill) drive the ball via `Goal::Objective` routes plus `Mover::Push`. The parked branch did not change that code. Its regression there is from the spawn change (below), not from the movers.

### Spawn placement (parked branch, v20 `getSpawnPoint`)
- Every spawn depends on this: players, bots, Slayer team spawns, and package test arenas. The v20 ray excludes the spawn brick itself, so on a spawn *plate* (Slayer team spawns and test fixtures use plates) the feet land 0.1 above the floor, inside the plate's own collision. v20's real spawn brick only has a thin pad, which Torque pushes the player out of. Our motor does not reliably do that from inside a full plate. Rule: never place feet below the spawn brick's own collision top under that point.

## What the cloud runs showed (2026-10-08 17:xx)
- Soccer at 0 goals came from spawn placement: feet were put inside Slayer's spawn plates. It is fixed by lifting feet to the spawn brick's own top. Slayer soccer passes (armed and unarmed).
- Grenades hurting their own side came from the sideways change. With it switched off, the grenade test passes. The lighter rule (the strafe carries on toward the spot's side after arriving) still fails it ("never a grenade near an enemy"), so grenade throwing depends on what the strafe does after the step-aside, not only on reaching it. **Decision for v0.2.7:** keep main's split (spots route the step-aside, the strafe runs after). Merging these needs the strafe to know about the shot (stand while the throw is clear), which is new behaviour rather than a cleanup, so it is not in this release.
- Landing in v0.2.7: the glimpse memory (fight/search flip-flops), the route bridge (search pauses), the full-width avoid for a failed step, and v20 spawn placement with the plate lift.

## Overlap 1 design: spots own the place, the strafe only moves within it
Why grenades depend on it: a thrower stands behind cover, and `spots` picks LEFT/RIGHT one body width out to get a clear throw. On arrival the anchor clears and the strafe runs free, with 3.5 s legs at 0.7 speed (several units). It often walks straight back behind the cover and throws into the wall beside its team. Both parked attempts kept that free strafe, which is why each failed.

Rule:
- **Spots own where it fights from.** The chosen place becomes `Brain::stand`: the anchor's point once reached, or where it stands when the chooser keeps HERE. It is set on each planning turn of a ranged fight and cleared outside one. The cover step-out stays an explicit LEFT/RIGHT spot, walked by the route at full speed, as on main.
- **The strafe owns lateral motion inside that place only.** A side is open only while the step keeps the bot within `STAND_ROOM` (half a body width) of `stand` along its right axis. At the edge the leg turns at once, using the strafe's existing turn-from-a-closed-side. It can never carry the bot one body width back behind the cover.
- **Moving elsewhere is a spot decision.** On its next planning turn the chooser weighs here/left/right/in/out from where the bot stands. A bigger move is a new spot and a new `stand`, never strafe drift.

Each behaviour, handled explicitly:
- Grenade and rocket cover-step: the route takes it out, and the strafe stays within half a width, so the line stays clear.
- Evasion: kept as weaving within the place, and the spot re-weighs every planning turn. Hit mid-leg it still turns at once.
- Edge, wall and ally checks: unchanged, combined with the room check.
- Melee footwork, swimmers and objective weaving: unaffected. They never set `stand` (ranged fights only).

## Movement ownership boundary (Max's trace, 17:53Z, confirmed in code)
`bot_safe_walk` steered: inside a live blast, `bot_blast_walk` replaced the chosen walk with "straight out". That made safety a second movement controller sitting above `act::resolve`. `bot_vehicle_detour` was also applied inside safety. Design:
- **Decide (one owner).** The proposals and `act::resolve`. Escaping a live blast is a movement decision, so it belongs to the threat owner, the dodge in `bot_extras`. That owner already handles projectiles in flight (`bot_incoming`) and now handles lying grenades too (`bot_blast_escape`). Standing in a blast, escape is all the extras do that tick, as a `Mover::Dodge` walk. No Stand is proposed, so nothing outranks it.
- **Execute.** The vehicle detour shapes the chosen walk round a vehicle the walk grid leaves out. It runs before safety, as route execution.
- **Validate (reject only).** Safety refuses a walk that enters a blast or goes no further out of one (`bot_blast_refuses`), a portal off the route, or a fall that hurts. It never invents a direction. The escape meets the same portal and edge checks as any walk.
- **What depends on it.** Bots still never walk into a lying grenade, still get out of one they stand in, and still stand at a deadly edge rather than escape over it (`the_way_out_of_a_blast_meets_the_edge_check`, now fed the dodge's escape walk). Both act.rs blast tests pinned the old steering mechanism. They are rewritten to the new owner with the same play outcomes asserted.
- **Diagnostics.** `acted.walk` names the deciding mover. The executed walk is the input the bot pressed (`BotThought.input`), which the clunk counter reads, so authority changes and reversals are measured on executed movement.

## Unstick (overlap 2): already one judge on main
Traced in code: on foot, `route::Progress::stalled` is the only stall judge. Its count drives the whole escalation: the first window hops (`stall_hop`), the second replans, and past `MAX_REPLANS` it gives up the goal. `wedged` and `spots`' `stuck` read that same judge's count or route outcome; they don't detect anything themselves. A jet leg's own failure feeds the same replan count. The other hops (a goof's, an extra's, into a body) are deliberate actions, not unstick paths. Vehicles have their own judge because the chassis is a separate controller. The "three detectors, two hop paths" note in next-version.md is from 2026-10-05 and is out of date. The trapped clock (respawn) is the last escalation. It is kept as it is: it needs a separate design for boxed-in spawns, and changing it now would be a behaviour change, not a cleanup.

## Overlap 1, revised after the cloud run (supersedes STAND_ROOM above)
Bounding the strafe to half a body width round the place passed grenades. But bots then barely moved in a fight: the arena test never once saw a bot see an enemy through the glass, where main sees many. That makes bots stationary, which is a gameplay regression, so it is replaced. Revised rule:
- `spots` owns where it fights from. A LEFT/RIGHT anchor records where the bot stood when choosing it (`Anchor::from`). On arrival the brain keeps `stepped_out`: where the step ended, and the flat way back.
- The strafe stays as free as on main, both ways and with the same legs, with one exception: it never goes back past `stepped_out` toward where it came from, so it cannot walk behind the cover it stepped out of. A place with no step-out (here, in, out, hop, jet) leaves the strafe exactly as main has it.
- `stepped_out` lasts while the bot fights from there, including later HERE choices. A new place or the end of its ranged fight clears it.
