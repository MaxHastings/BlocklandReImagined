> **Historical note. Written 2026-10-05 (the file's date in the cloud project folder, `bots/npc-edge-cases.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# NPC edge cases and simplification audit

Read-only audit, 2026-10-05. Repo `/home/claude/blocklandreimagined`, branch
`claude/project-thread-pt64ji` (abd81c4e). Nothing was built or run.

**Lanes read** (none checked out):

- `fix/bot-surprise`: committed, 9fcaea76.
- `fix/bot-route-planner`: committed, 7af9aa9a, plus WIP 7177637e.
- `fix/bots-ball-games`, `fix/bot-team-roles` and `fix/bot-perception`: these
  exist only as local branches with **uncommitted** worktree changes, which I
  read with `git -C <worktree> diff`:
  - ball-games: `.claude/worktrees/agent-a50c53f2e4a8b5c5b`
  - team-roles: `.claude/worktrees/agent-aeef4416cd1271368`. It already merges
    surprise, and adds `bots/team.rs` (524 lines) and `bot_kind/team.rs`.
  - perception: `.claude/worktrees/agent-aabf96c2f0e8ebadf`. It adds
    `bots/perception.rs` (673 lines, untracked) and a `Blast` weapon event.

Line numbers in lane files are approximate because the files are uncommitted.

**Status words.** **H** means Handled, **P** means Partly handled, **N** means
Not handled, and **W:lane** means it will be handled by that lane. *(inferred)*
marks a status I reasoned from surrounding code rather than reading the exact
path.

---

## 0. Smallest element set, and why each part exists

**Smallest element set:** these seven parts are the whole design. All variety
comes from richer inputs to them, not from more parts.

1. **Perception**: what it sees, glances at, and the dated evidence it keeps.
2. **Options**: behaviours, targets, weapons, aim points, routes, flavours and
   affordances.
3. **Chooser**: worth − cost + uses + mood + pressure, picking from the
   near-best band with drift, boredom and an effectiveness signal.
4. **Hold rule**: a minimum commit time plus hysteresis, with the real
   interrupts.
5. **Planners**: the route planner (the legs) and the rule/objective planner
   (the steps).
6. **Executors**: ordinary player controls.
7. **Claims**: exclusive reservations plus published intents.

Rule for verdicts: **few mechanisms, rich inputs.** A duplicate timer, scorer
or bookkeeping mechanism is merged or cut. A source of variety (options,
actions, interplay terms) is **Keep: variety**.

| # | Part (file) | Job nothing else does | What visibly breaks or goes dull without it | Size (lines / tunables) | Verdict |
|---|---|---|---|---|---|
| 1 | Sight + dated evidence (`bots.rs:1263-1318` `bot_sight`, `Knowledge`, `Brain.memory`) | Decides who is a seen enemy, and keeps dated last-seen positions without reading hidden positions | Bots become omniscient or blind | ~120 / `sight`, `memory_seconds` | **Keep** |
| 2 | Hurt map + `objective_threat` (`bots.rs:150`, `:245`, `:545-561`, `:1838-1850`) | Weak: a second and third evidence slot holding the same `Knowledge` type as `memory` | Nothing if folded into one evidence list tagged "hurt me" | ~40 / 0 | **Merge into 1** |
| 3 | `search_memory.rs` | Probes unchecked space around the last-seen point along the observed heading | Bots stand still at the last-seen spot | 199 / 0 | **Keep** |
| 4 | Alerts (`hear_alerts`, `bots.rs:1527-1571`) | Ally evidence, without extending its age (design item 5) | Teammates do not converge on a seen enemy | 45 / `alerts_allies` | **Keep** |
| 5 | Behaviour list + base scores (`behaviour.rs` `score`) | The option list and its base worth | No options | 361 (incl. tests) / `behaviours` weights | **Keep** as the option source. Its argmax `choose` merges into 7 (the surprise lane already splits `scores`/`best`) |
| 6 | Per-transition hysteresis: Fight band slack (`behaviour.rs` Fight, `fighting.band_slack`, `min_band_slack`); Fight→Chase dwell (`bots.rs:2101-2108`, `dwell_seconds`); Return "walks all the way"; ball-games `pursuing` flag, `fly_rise` and `fly_drop`; tactics weapon hysteresis (`tactics.rs` test `hysteresis_and_stable_ties_stop_inventory_flip_flop`) | Weak: each re-does what the hold rule does, for one pair of behaviours | Nothing once the hold rule exists | ~80 / 6 | **Merge into 8** |
| 7 | Surprise chooser (`fix/bot-surprise` `bots/surprise.rs` `Mind::pick`, `bot_kind/surprise.rs`) | Weighted pick among near-best options with drift, per-option boredom and effectiveness | Bots always do the single best thing, so they are predictable | 1691 + 134 / 18 + 11 flavour weights | **Keep**. Add per-kind boredom (play vs flavour). The tell merges into 10 |
| 8 | Hold rule (surprise `commit_seconds` plus gates) | One commit time with real interrupts | Flip-flopping, or (with patches) many special cases | inside 7 / 1-4 | **Keep**, and make it apply at strength 0 too. The surprise `Gate` (urgent/carrying), ball-games `committed`/`gunning` and `pausing_delivery` (`bots.rs:1866-1880`) become its interrupt list |
| 9 | Surprise flavour interrupts (11 kinds) | Idle player actions: emote, spray, hop, light and so on | Bots look robotic when idle | in 7 / 4 + 11 | **Keep: variety**. Make flavour an option kind in 7 rather than a separate gate (see O15) |
| 10 | Surprise tell (`tell_seconds`) | Telegraphs a switch | Switches look instant | ~40 / 1 | **Merge into 12** (it is a reaction delay) |
| 11 | Team intents + terms (`fix/bot-team-roles` `bots/team.rs`, intents in `claims.rs`) | Crowding (overlap), uses, harm volumes, mood pull | Bots clump, walk through each other's fire, never ride a teammate's jeep | 524 + 69 + 62 / 10 + callouts | **Keep: variety** for the terms. Intents already live in claims, which is good. Its `uses` seat bonus merges with 15's crew utilities |
| 12 | Perception lane (`bots/perception.rs`, `BotPerception`) | Glances at blasts, sounds, gaze and motion; reaction delay and wobble scaled by alertness | Bots ignore rockets landing next to them and react with the same speed when idle | 673 + 75 / 21 | **Keep: variety** for glance sources. **Merge** the reaction and wobble model with the existing `reaction_seconds`, `aim_error_degrees` and `ERROR_TICKS` (`bots.rs:2475-2499`) into one model. Cut to ~12 tunables (one reach per source, not two or three) |
| 13 | Claims (`claims.rs`) | Exclusive advisory reservations (seats; bodies among allies), leases and failure cooldown | Two bots take one seat; allies shove the same ball | 418 / 0 | **Keep** |
| 14 | Contest (`contest.rs`) | Lead a moving body, clear a body driven back, work it off walls, cover a teammate | Ball play looks dumb: bots get carried back, press the ball into walls, stand idle | 402 / 6 | **Keep: variety** (these are actions). Whether to cover becomes a chooser option |
| 15 | Interactions (`interactions.rs`) | Seat/body opportunities, boarding, seat control adapter, local sidestep | No vehicles, no hazard pushes | 1170 / `interact` | **Keep** the executors. **Merge** the seat utilities (0.84/0.88/0.72, `interactions.rs:318-328`) into the chooser's uses term and the route planner's board leg |
| 16 | Ally-geometry helpers: `bot_ally_corridor` (`interactions.rs:538-584`); strafe `allies_near` (`bots.rs:2408-2417`); `bot_walk_direction` (`interactions.rs:710-745`); `bot_fire_clear` (`interactions.rs:859`); team `inside`/`exit` | Weak: five versions of "is an ally in this volume" | Nothing if one helper serves all five | ~200 / 0 | **Merge** into one ally-volume helper |
| 17 | Objective stack: `objectives.rs`, `planning.rs`, `physical_objectives.rs`, `package_objectives.rs`, `combat_objectives.rs` | Reads rules into desired states and plans grounded steps (the frozen contract) | No soccer, CTF or races | 2616 + 1506 + 1041 + 521 + 391 / `objective_radius` | **Keep** |
| 18 | Hand combat + tactics + charged control (`combat.rs`, `tactics.rs`, `charged_control.rs`) | Inventory attack choice, ballistic intercepts, safe blast, charge handling | Bots miss with rockets or blow themselves up | 1176 + 1538 + 131 / 0 | **Keep: variety** (weapons are options). Its own hysteresis merges into 8 |
| 19 | Legacy `bot_weapon` / `Weapon::band` (`bots.rs:1387-1461`, `:484-513`) | Fallback reach and band for images tactics cannot model | Unknown Add-On weapons stop firing | ~110 / 0 | **Keep** for now. **Merge** into the tactics capability later |
| 20 | Arming (`arming.rs`) | Walk to a weapon when it has no attack | Unarmed bots hug enemies | 118 / `arm` weight | **Keep: variety**. Its `skipped` list (TIMEOUT/AT_ITEM/SKIP) merges with `claims.failures`, which does the same "cool down a failed target" job |
| 21 | Fly behaviour + `air_chase` + `AirChase` (`bots.rs:1320-1347`, `:2233-2242`, `:2627-2705`) + ball-games `fly_*` (3 tunables) | Weak: superseded. The route planner's jet legs do this | Nothing once the route planner lands | ~140 / 3 | **Cut** (the route-planner lane already deletes it; ball-games should drop its Fly changes) |
| 22 | Swim-kind shortcut (`bots.rs:2295-2320`) | Keeps `moves: swim` creatures in their water | Sharks beach themselves | ~30 / `moves` | **Keep** (package policy, kept by the route planner) |
| 23 | Driver reversing: `reverses()` + `mounted.reverse_degrees`/`reverse_distance` (`interactions.rs:1141`) vs `route::gear` (route-planner lane) | Weak: two reversing policies | Nothing once one remains | ~40 / 2 | **Merge into route::gear** (keep `reverse_distance` as its pursuit limit). The lane is based on an older base and deletes code that is no longer there: a merge conflict |
| 24 | Progress watchdogs: walk `stuck`/`STUCK_TICKS`/`MAX_REPLANS` (`bots.rs:2711-2737`); ball-games `wedged`; `vehicle_stuck` at 360/480/720 (`interactions.rs:1059-1099`); objective `APPROACH_TIMEOUT`; arming TIMEOUT; search probe timeouts; fly give-up | Weak: eight versions of "no progress for N ticks, then give up" | Nothing if the planner owns one progress monitor per leg (its own design text says so) and claims keep the lease | ~150 / 3 | **Merge into the planner** (medium risk; many tests touch it) |
| 25 | Carry behaviour (`bots.rs` Carry branch, `open_spot` `:1349-1385`, CARRY/LIFT/SWING/REGRAB) | Gravity-gun grab, carry to open space and fling | The gravity gun is never used for fun throws | ~120 / 0 | **Keep: variety**. Simplify `open_spot` (up to ~800 rays in one tick, Pf4): fling where it stands if open, otherwise use the planner's nearest open cell |
| 26 | Weave and strafe timers: `strafe_seconds`, plus `(tick/90+bot)` weave twice (`bots.rs:2555`, `:2670`) | Weak: three dodge patterns | Nothing if one strafe pattern serves Fight, Objective and Fly | ~40 / 1 | **Merge** |
| 27 | Three RNGs: `Brain::random` (`bots.rs:393`), `Mind::random` (surprise), `perception::draw` | Weak | Nothing; one per-bot stream suffices | ~25 / 0 | **Merge** (low risk; changes seeded outcomes, so test seeds shift) |
| 28 | Return behaviour + leash (`Brain::pursuit`) | Brick bots stay near their brick | Guards wander off | ~30 / `wander_radius`, `chase_radius`, `mounted.*` | **Keep** the option. **Cut** ball-games' `pursuing` patch (the hold rule covers it) |
| 29 | Ball-games `committed`/`gunning` score constants (0.92/0.65) | Run-and-gun and carrier focus | Carriers stop to fight | ~60 / 0 | **Keep: variety** for the fire-while-moving action. **Merge** the constants into the chooser's worth (a carrier's objective worth includes what it holds) |
| 30 | Route planner (`fix/bot-route-planner` `route.rs`, nav legs) | One search over walk/jump/crawl/portal/swim/jet edges plus board/drive legs (design item 6) | Fly, swim and drive special cases stay | 591 + ~490 nav / `fly` weight | **Keep** |
| 31 | Surprise flank routes (`Domain::Route`, `flank_distance`) | Alternative chase lines | Every chase is a beeline | in 7 / 1 | **Keep: variety**. Generate the alternatives from the planner (k-best routes) rather than fixed offsets |
| 32 | Surprise splash aims (feet, nearby surface) | Rocket splash play | Rockets always go for the torso and get dodged | in 7 + combat.rs +194 / 0 | **Keep: variety** |
| 33 | Diagnostics: `BotThought`, `BotSurpriseView`, `BotTeamView`, `BotNotice` | Why-views for headless tests | Can't explain behaviour | ~250 / 0 | **Merge** into one decision record per tick (each lane added its own view) |
| 34 | Team callouts (`team.rs` `callout`, `team_say`) | Chat line when a teammate's intent changed its choice | Coordination is invisible on camera | ~40 / 1 + lines | **Keep: variety** (cheap) |

### Cheap ways to widen the option space (no new mechanisms)

- **Let Interact run without an enemy.** `interactions.rs:419-426` returns no
  opportunity when `enemy.is_none()`. Remove that gate for body pushes and
  passenger seats, scored as the flavour kind. Idle bots would then kick balls,
  push crates and ride along in a teammate's jeep. Folds into: Chooser.
- **Head aim.** Add a head aim point read from the hit-region data
  (`Session::region_of`, `combat.rs:1010`) next to torso, feet and surface.
  Folds into: Chooser (Aim domain).
- **Crouching as a survival option.** Damage already has crouch scaling
  (`combat.rs:1015`), so crouching under fire can be scored as an option.
  Folds into: Chooser.
- **Jet hop as a dodge** when hurt and the kind can jet, using ordinary
  controls. Folds into: route planner jet leg, short form.
- **Brick activation as flavour.** Bricks with `onActivate` rows, through the
  existing captured-input ledger, give flavour options: light switches, doors,
  music bricks. Folds into: Options + Objective input ledger.
- **k-best routes** from the planner as Route options instead of fixed flank
  offsets. Folds into: Planner + Chooser.
- **Hand a spare weapon to an unarmed teammate**: drop the tool toward them,
  scored by the team `uses` term (the drop flavour already exists). Folds into:
  team terms.
- **Passenger seat as flavour** (riding along), already discoverable from seat
  data. Folds into: Interactions + Chooser.

---

## 1. Summary

**Counts (118 edge cases):**

| Status | Count |
|---|---|
| Handled | 69 |
| Partly handled | 26 |
| Will be handled by a lane | 18 |
| Not handled | 5 |

Every proposed fix below folds into an existing part. **None needs a new
element.**

**Defaults to know before a showcase.** Surprise ships at `strength: 0`
(`bots.json` in `fix/bot-surprise`) and team weights default to 0
(`BotTeam::default`; that lane does not edit `bots.json`). Only perception is
switched on for the Blockhead in its worktree. As shipped, a video shows no
variety and no coordination until those values are tuned.

### Top 10 gaps, ranked by how likely they show up on camera

Showcase scenes: bot soccer, TDM with rockets, tanks, goofing.

| Rank | Gap | Scene | Proposed fix (folds into) | Owner lane |
|---|---|---|---|---|
| 1 | **Bots never goof in objective games** (O15, F2). Flavour interrupts need a "natural pause": no objective, no enemy, no memory. In soccer or CTF that never happens, and surprise is off by default anyway. | goofing, soccer | Make flavour an option kind scored by the chooser, with per-kind boredom: play gets boring while a teammate has the ball or the bot is covering. Mood pull works on both kinds. Turn strength on for the Blockhead. (Chooser) | fix/bot-surprise |
| 2 | **Own goals are tolerated up to half of all goals** (O7). The soccer gate is `own_goals * 2 <= total` (`bot_soccer_match.rs:1004`, `:1021-1023`). | soccer | Add a cost term on push approaches: an approach whose contact line points the ball within the defended goal's mouth costs the expected own goal. Widen `push_approach`'s arc radius when goal-side. Tighten the gate to about 1 in 5. (Chooser cost + contest/push_approach) | fix/bots-ball-games |
| 3 | **Jeeps and tanks run over teammates** (T8). `a_jeep_on_each_side` fails with team kills 2 and stuck 9.9% (`docs/progress/2026-10-05-bot-surprise.md`, on `fix/bot-surprise`). | tanks | The drive leg treats the chassis's swept stopping path as a harm volume and brakes or steers out of allies, reusing the team lane's `inside()` and the merged ally-volume helper (part 16). (Planner + ally volumes) | fix/bot-route-planner |
| 4 | **Pile-on** (F5). Every bot targets the nearest visible enemy (`bots.rs:1313`). The team overlap term only lowers the Fight *behaviour*, so a crowded bot switches to Chase or Wander, never to a different enemy (`team.rs` `terms`). | TDM | Make each visible enemy its own Fight/Chase option, so the overlap cost spreads targets. (Options + Chooser) | fix/bot-team-roles |
| 5 | **360-degree vision** (P2). `bot_sight` has no view cone (`bots.rs:1263-1318`, `simulation.rs:1652`), so bots turn instantly on enemies behind them. | TDM | Perception's `away_scale` already slows the reaction. Also slow the turn toward an unglanced target behind the bot. A cone is optional; a slower reaction is enough. (Perception) | fix/bot-perception |
| 6 | **Spawn-protection insta-kill** (I8, A7). Bots aim at a protected player and hold fire (`combat.rs:573`, `:898`). The reaction clock started at first sight (`bots.rs:2496`), so they fire the frame protection ends. | TDM | Start the reaction clock when the target becomes damageable. Give a protected target low worth so the bot keeps moving. (Perception + Chooser) | fix/bot-perception |
| 7 | **Fast oscillation when surprise is off** (F3). The plain brain has only per-pair patches (part 6). Commitment exists only inside surprise at strength > 0. | all | One hold rule in the chooser at every strength: a minimum commit time plus a hysteresis margin. Interrupts: real damage, objective pickup/drop, target lost/dead, affordance lost. (Hold rule) | fix/bot-surprise |
| 8 | **Wrecked or flipped tank crew** (V2, V5). A destroyed vehicle keeps its occupants (`vehicles/src/world.rs:1328-1335` zeroes controls only). Bots keep "driving" until `vehicle_stuck >= 720` (6 s, `interactions.rs:1090`). There is no flip check. | tanks | "Affordance lost" becomes a hold-rule interrupt: in `observe_bot_objects`, a destroyed vehicle or one whose hull up is below cos 60° ends the drive leg now (leave leg). Use native Activate to flip where permitted. (Hold rule + Planner) | fix/bot-route-planner |
| 9 | **Harmless chasing** (N2, I2). A melee bot stands under an enemy on a roof forever (Fight needs rise ≤ step+1). An unarmed bot with nothing to pick up still Fights/Chases with band (2,3) and never hits (`bots.rs:2056-2060`). | TDM | Chooser worth: Fight/Chase are worth 0 when `!can_retaliate` (already computed at `bots.rs:1859-1865`), or when the planner's best route ends beyond reach. Search, Return or flavour then win. (Chooser + Planner cost) | fix/bot-route-planner |
| 10 | **No objective pressure** (O12, T3). Nothing raises play when the team is behind or short-handed (design item 8). No lane implements it. | soccer, CTF | Multiply objective worth by 1 + `pressure` × clamp(deficit). The deficit comes from the already computed `team_score` (`objectives.rs:708-717`) and team sizes. One tunable. (Chooser) | fix/bot-team-roles (no lane owns it; suggest this one) |

Just outside the top 10: in an FFA mini-game with no teams, one builder's
spawn-brick bots are all allies (T1, `bots.rs:1253-1261`), so they never fight
each other. Also, team-lane intents are read whether or not the teammate is
visible (T9).

### Lane integration conflicts (not counted above)

- **Fly.** Ball-games adds `fly_rise`/`fly_drop`/`fly_give_up_seconds` and
  changes `air_chase`. Route-planner deletes the Fly behaviour. Keep the
  planner's version and drop ball-games' Fly changes.
- **Reversing.** Route-planner replaces `error.abs() > 1.8` (an older base)
  with `route::gear`. The current branch has `reverses()` with
  `mounted.reverse_*`. Expect a conflict, and keep one policy (part 23).
- **Base commits.** Surprise is based on ball-games 2fcb7b89, and team-roles
  merges surprise. Route-planner's base is older. Each lane edits
  `bots.rs` `step_bot` heavily, so the land order matters.

---

## 2. Edge cases by category

Each row gives what should happen, the status with evidence, and, for Partly
or Not handled, the smallest fix and which existing part it folds into.

### Lifecycle

| # | Edge case | What should happen | Status | Evidence | Smallest fix (folds into) |
|---|---|---|---|---|---|
| L1 | Spawn from a brick | Bot appears by its brick in its kind's body, on the brick's team | H | `bots.rs:640-712`, `:1040-1086`; tests `bots_of_one_builder_are_on_one_side`, `a_spawn_bricks_team_choice_puts_its_bot_on_that_team_across_save_and_load` | |
| L2 | Spawn over the 16-bot cap | Refused, and the builder is told | H | `bots.rs:669-678`; `MAX_BOTS=16` (`package-runtime/src/ops/mod.rs:488`); test `a_bot_over_the_server_limit_tells_its_builder` | |
| L3 | Death mid-action (charge, claim, seat, objective) | Everything let go, nothing fired | H | `bots.rs:1600-1636`; tests `a_dead_driver_releases_its_seat_without_removing_the_living_gunner`, `disconnecting_the_charge_target_cancels_without_a_stale_throw` | |
| L4 | Respawn after a mini-game reset | New life, but learned preferences and the random stream carry on | P | Death respawn keeps the surprise `Mind` (`new_life`), but `respawn_brick_bot` builds a fresh `Brain` (`bots.rs:735`), resetting the rng (`:345`) and all surprise drives. Each round replays from the same seed. | Copy `rng` and `surprise` across, as `sequence` and `named` already are (`bots.rs:736-740`). (Brain lifecycle) Owner: fix/bot-surprise |
| L5 | Team switch mid-life | Old hostility, search and plan dropped | H | `bots.rs:1587-1596`; objective `desired_context` (`objectives.rs:1686-1690`); tests `becoming_an_ally_invalidates_remembered_hostility_before_expiry`, `a_real_team_change_invalidates_approach_and_due_time_success_guards` | |
| L6 | Builder leaves; rules game ends | Brick bot leaves the game and is harmless; rules bot leaves with its game | H | `bots.rs:1087-1131`, `:970-983`; test `removing_a_rules_bot_releases_its_reservation_before_the_lease_expires` | |
| L7 | Admin kicks a bot | Bot removed cleanly (or kicking is refused) | P *(inferred)* | Kicks go through `connection_to_owner` (`admin.rs:476-478`); bots have no connection, so they cannot be kicked. A peer that vanishes leaves a brain that `step_bot` skips until a reconcile. | Route a kick of a bot owner to `drop_bot` (`bots.rs:797`). (Lifecycle) Owner: root |
| L8 | Mini-game reset / round end | Fresh life at the brick, plans tied to the old round dropped | H | `respawn_brick_bot` `bots.rs:714-749`; failed-plan round check `objectives.rs:1693-1706`; tests `recipe_goals_bots_contest_one_ball_score_and_play_on_after_the_reset`, `team_sides_and_goal_guards_survive_save_and_reset` | |
| L9 | Map change / build load | Bots follow bricks; team applied after load | H | reconcile + `apply_brick_team`; tests `the_shipped_soccer_save_loads_a_ready_match`, `the_shipped_field_survives_load_bricks_whatever_the_pads_size` | |
| L10 | Many bots (16) | Bounded per-tick cost | H | budgets in Pf1; tests `profile_sixteen_bots_in_the_real_bedroom`, `sixteen_objective_controllers_and_mixed_inventory_combat` | |
| L11 | Zero bots | No cost | H | `bots.rs:1464-1470` (drops navs, stops collision tracking) | |
| L12 | Only bots, no humans | Rules bots play on; brick bots need their builder in the game | H *(inferred)* | `sync_bot_minigames` `bots.rs:1087-1131` | |
| L13 | Bot kind's Add-On disabled | Its bots leave | H *(inferred)* | `set_bot_kinds` `bots.rs:600-617` | |
| L14 | Rules hold a bot still (rest) | It stands, fire released, claims and seat let go | H | `bots.rs:923-955`, `:1637-1656`; tests `rules_rest_releases_a_driver_and_resume_can_make_a_new_decision`, `a_resting_failed_model_does_not_starve_other_ready_controllers` | |
| L15 | Lane state across death (published intents) | A dead bot's intent stops steering teammates | W:fix/bot-team-roles | `claims.forget(bot)` in the death branch (team worktree `bots.rs` diff), and intents lapse after 3 ticks | |

### Teams and allegiance

| # | Edge case | What should happen | Status | Evidence | Smallest fix (folds into) |
|---|---|---|---|---|---|
| T1 | FFA mini-game (no teams) with one builder's brick bots | Bots of one builder stay one side; others fight | H | Decided 2026-10-05 (stabilization): no change. v20's minigameCanDamage treats two bots of one brick owner as that owner's own, so they never hurt each other; test `bots_of_one_builder_are_on_one_side` (T6) is the faithful rule. Bots of different builders, and Slayer's rules bots, do fight. | |
| T2 | A single team | Nothing to fight; objective or wander | H *(inferred)* | `bot_team_relation` `bots.rs:1219-1233` | |
| T3 | Uneven teams | The short side leans on the objective | N | nothing reads team size | See O12. (Chooser pressure term) Owner: fix/bot-team-roles |
| T4 | Friendly fire on | Teammates are still not enemies | H | `bots.rs:1235-1240`; test `rules_bots_do_not_target_same_team_humans_or_bots_with_friendly_fire_on` | |
| T5 | Ally in the line of fire / in a splash | Hold fire; never self- or team-splash | H | `bot_fire_clear` `interactions.rs:859`; `safe_blast` `combat.rs:629`; gauntlet reports zero team kills (surprise progress note) | |
| T6 | Same-builder brick bots | On one side | H | test `bots_of_one_builder_are_on_one_side` | |
| T7 | Human joins mid-round | Becomes a target or ally at once | H *(inferred)* | relations read live each tick (`bots.rs:1194-1261`) | |
| T8 | Driver runs over teammates | Brake or steer out | P | `bot_ally_corridor` braking `interactions.rs:1046-1048`, test `a_driver_brakes_for_a_grounded_ally_before_forward_or_reverse_contact`; still `a_jeep_on_each_side` team kills 2 | Swept stopping path as an ally harm volume in the drive leg (top-10 #3). (Planner + ally volumes) Owner: fix/bot-route-planner |
| T9 | Teammate intents read when not visible | Only intents the bot could see or hear count (design item 4) | P | `team_intents` filters allies only (`team.rs` ~line 222) | Filter by alert reach or the existing sight test. (Perception) Owner: fix/bot-team-roles |
| T10 | Target becomes an ally mid-charge | Charge cancelled without a throw | H | test `becoming_an_ally_cancels_an_armed_hand_spear_without_throwing_it` | |

### Objectives and rules

| # | Edge case | What should happen | Status | Evidence | Smallest fix (folds into) |
|---|---|---|---|---|---|
| O1 | No objective | Fight, wander, flavour | H | `bot_objective` returns None (`objectives.rs:1659-1669`) | |
| O2 | Unreadable or unsupported rules | Plan marked unsupported and explained; no fake goal | H | `objectives.rs:1890-1905`; tests `an_unknown_collateral_effect_rejects_the_complete_action`, `an_unmodelled_named_target_reaction_rejects_the_whole_objective_action` | |
| O3 | Rules too big for the budgets | Explicit bounded failure | H | tests `too_many_real_action_sources_report_the_specific_grounding_bound`, `a_sixteen_step_dependency_is_an_explicit_depth_negative_not_partial_success` | |
| O4 | Moving objective (ball) | Lead it; stay engaged against an opponent | H | `contest.rs:55-88`, `physical_objectives.rs:689-692`; test `recipe_goals_bots_contest_one_ball_score_and_play_on_after_the_reset` | |
| O5 | Objective destroyed or reset | Replan at once for the new one | H | `objectives.rs:1747-1750`; test `ordinary_respawn_replaces_the_carried_source_without_completing_the_old_journey` | |
| O6 | Both teams score the same goal | Each side plans for its own team score | H *(inferred)* | team-scoped score keys `objectives.rs:670`, `:719-760` | |
| O7 | Own goal | Approach from the right side; very rare own goals | P | heading to own scoring region `physical_objectives.rs:695-700`; arc approach `interactions.rs:24-74`; test `a_bot_scores_in_its_own_teams_goal_after_save_and_reset`; but the gate allows own goals up to half (`bot_soccer_match.rs:1004`, `:1021-1023`) | Top-10 #2. (Chooser cost + push_approach) Owner: fix/bots-ball-games |
| O8 | Gap between two steps | No blink to Wander | W:fix/bots-ball-games | worktree docs diff: keeps the finished step's view up to 1 s | |
| O9 | Failed step cooling down | Not a walk home | W:fix/bots-ball-games | `Situation.pursuing` (worktree `behaviour.rs`) | Better as a hold-rule case than a flag (part 28) |
| O10 | Carrier meets an enemy | Keep delivering and shoot on the way | W:fix/bots-ball-games | `committed` 0.92 / `gunning` (worktree `behaviour.rs`, `bots.rs`) | Move the constants into chooser worth (part 29) |
| O11 | Long route to the objective | No false timeout | W:fix/bots-ball-games | deadline moves with progress (worktree docs diff) | |
| O12 | Team behind (objective pressure, design item 8) | Play options gain worth | N | no code in any lane | Pressure term from `team_score` `objectives.rs:708-717` (top-10 #10). (Chooser) Owner: fix/bot-team-roles |
| O13 | Carrier is attacked | Real damage may interrupt; a passive bystander does not | H | `bots.rs:1866-1880`; tests `an_actual_attacker_can_interrupt_a_retained_delivery`, `an_armed_bot_retains_delivery_instead_of_attacking_a_passive_visible_creator` | |
| O14 | Every bot replans after a goal | Fair turns, nobody starves | H | one plan per tick `objectives.rs:1795-1801`; test `round_robin_does_not_starve_same_modulo_owner_ids` | |
| O15 | Goofing during an objective game | Occasional flavour when its play matters little (a teammate has the ball, it is covering) | N | `natural` pause requires no objective (surprise lane `bots.rs`; `surprise.rs` `interrupt`) | Top-10 #1: flavour as an option kind with per-kind boredom. (Chooser) Owner: fix/bot-surprise |

### Inventory

| # | Edge case | What should happen | Status | Evidence | Smallest fix (folds into) |
|---|---|---|---|---|---|
| I1 | No weapon, one lying nearby | Go and pick it up | H | `arming.rs`; gauntlet `weapons_lying_on_the_ground`; test `an_empty_handed_bot_arms_before_its_objective` (`behaviour.rs`) | |
| I2 | Empty inventory, nothing to pick up, no melee | Do not chase an enemy it cannot hurt | P | band (2,3) with no weapon (`bots.rs:2056-2060`); never fires | Fight/Chase worth 0 when `!can_retaliate` (`bots.rs:1859-1865`). (Chooser) Owner: fix/bot-surprise |
| I3 | Ammo out | Switch to a usable slot | H | test `a_depleted_stored_magazine_switches_to_the_usable_undrawn_slot` | |
| I4 | Weapon cannot hurt the target (sheltered in a vehicle, armour) | Switch weapon or aim, or move | P | Non-damageable players are not enemies (`bots.rs:1215`). The sheltered case relies on the effectiveness signal | W:fix/bot-surprise effectiveness (shot judged after flight). (Chooser) |
| I5 | Melee only | Close to its band and swing | H | `bots.rs:2569-2576`; test `an_empty_handed_bot_that_bites_closes_in_and_bites_once_a_second` | |
| I6 | Explosive self-damage | Never in its own blast | H | `safe_blast` `combat.rs:629`; band near = splash+3 (`bots.rs:502-513`); tests `nearby_blast_risk_selects_the_safe_second_inventory_slot`, `a_close_ranged_flyer_escapes_a_low_roof_and_delivers_a_safe_blast` | |
| I7 | Charged weapon and the target is lost | Cancel without a stale throw | H | tests `a_moving_target_keeps_native_windup_until_a_valid_throw`, `an_already_released_charge_cancels_when_its_target_disconnects` | |
| I8 | Target is spawn-protected | Do not camp and insta-fire | P | `combat.rs:573`, `:898` withhold; reaction clock `bots.rs:2496` started earlier | Top-10 #6. (Perception + Chooser) Owner: fix/bot-perception |
| I9 | Flavour "drop" throws away its only weapon | Never with a single attack | W:fix/bot-surprise | `spare_weapon` requires ≥2 attacks (`surprise.rs` `surprise_pause`) | |

### Vehicles

| # | Edge case | What should happen | Status | Evidence | Smallest fix (folds into) |
|---|---|---|---|---|---|
| V1 | Empty vehicle, no enemy | May be used to travel (route leg) or for flavour | P | Interact needs an enemy (`interactions.rs:419-426`). The route lane's `drive_serves` is also enemy-only | Let the planner offer board/drive for any far goal. Lift the enemy gate for seats (widening list). (Planner) Owner: fix/bot-route-planner |
| V2 | Vehicle destroyed while seated | Leave the wreck at once | P | claims released `interactions.rs:160-166`; occupants not ejected (`vehicles/src/world.rs:1328-1335`, inferred); bot leaves only after `vehicle_stuck >= 720` or a Wander tick | Affordance-lost interrupt (top-10 #8). (Hold rule) Owner: fix/bot-route-planner |
| V3 | Seat contention, bots and humans | Exclusive seats; humans win | H | `claims.rs:24-33`; tests `a_human_occupant_wins_over_intentions_and_hostile_bots_do_not_join_their_crew`, `two_allied_bots_contend_for_the_same_body_without_duplicate_controllers_or_awards` | |
| V4 | Driver with no route | Replan, back up, give up the seat | H | `interactions.rs:1074-1099`; test `a_chassis_replans_or_stops_at_a_gap_only_a_pedestrian_can_fit` | |
| V5 | Flipped vehicle | Get out (or flip it) quickly | P | no upright check; treated as stuck (6 s) | Hull-up check ends the drive leg (top-10 #8). (Planner) Owner: fix/bot-route-planner |
| V6 | Water | Chassis never routes through water; watercraft unsupported | W:fix/bot-route-planner *(inferred)* | `Body.swims: false` for chassis (route lane `interactions.rs` diff); `bots.md` lists watercraft as unsupported | |
| V7 | Gunner with no driver | Gets out after a wait | H | `interactions.rs:1100-1112`; test `a_lone_driver_does_not_wait_forever_for_an_absent_gunner` | |
| V8 | Tank charged gun | Hold until ready, then release | H | test `a_charged_mounted_weapon_is_held_until_ready_then_released_and_rearmed` | |
| V9 | Reversing policy | One rule for backing up | W:fix/bot-route-planner | `route::gear` (route lane) vs `reverses()` `interactions.rs:1141` (merge conflict, part 23) | |
| V10 | Chasing past the walking leash while mounted | Mounted leash applies | H | `Brain::pursuit` `bots.rs:327-334`; test `a_mounted_driver_keeps_closing_on_a_retreating_target_beyond_its_walking_leash` | |
| V11 | Boarding a moving vehicle | Refused above 2 m/s | H | `interactions.rs:600-608` | |

### Navigation

| # | Edge case | What should happen | Status | Evidence | Smallest fix (folds into) |
|---|---|---|---|---|---|
| N1 | Unreachable objective | Bounded failure, try an alternative | H | partial route `bots.rs:2346-2359`; tests `an_unreachable_sensor_produces_a_bounded_failure_without_success`, `inaccessible_cheaper_region_cools_down_and_the_reachable_alternative_wins` | |
| N2 | Enemy on a roof it cannot reach | Shoot if it can; otherwise go elsewhere | P | Fight needs rise ≤ step+1 (`behaviour.rs` Fight); a melee bot chases underneath forever | Chase worth 0 when the planner's best route ends beyond reach (top-10 #9). (Planner cost + Chooser) Owner: fix/bot-route-planner |
| N3 | Enemy high up, kind can jet | Jet up and over | W:fix/bot-route-planner | jet legs replace Fly; today test `a_jetting_bot_closes_on_an_enemy_on_a_high_brick_platform` (Fly). Ball-games' fly give-up conflicts | Keep only the planner version |
| N4 | Moving platforms / standing on a body | Get off and replan | P | grid only learns brick collision changes (`bots.rs:1471-1478`); ball-games walks off a body (worktree `bots.rs`) | Planner: a "floor not on grid" check ends the leg (the ball-games code moves there). (Planner) Owner: fix/bot-route-planner |
| N5 | Portals | See, walk and shoot through | H | tests `a_bot_sees_its_enemy_through_a_portal_and_walks_through_after_them`, `an_armed_bot_shoots_through_a_portal_from_where_it_stands` | |
| N6 | Deep water for a walker | Swim across or around | W:fix/bot-route-planner | swim edges in `route.rs` `Swim`; gauntlet `water_between_the_sides` | |
| N7 | Falling, drops, ledges | Walk off cleanly; no edge wobble | H | `bots.rs:2378-2390`; gauntlet `rooftop_brawl_without_rails` | |
| N8 | Stuck detection | Hop, replan, give up | H | `bots.rs:2711-2737`; ball-games `wedged`; gauntlet `stairs_to_a_deck` | Merge watchdogs (part 24) |
| N9 | Narrow door, many bots | Queue or flow through | P | everyone sidesteps left (`interactions.rs:710-745`); test `a_rotated_off_grid_narrow_door_is_passable_by_human_and_pursuing_bot` (one bot) | An ally ahead going the same way: follow at its pace instead of sidestepping. The team overlap term on place spreads goals. (Planner executor + Chooser) Owner: fix/bot-route-planner |
| N10 | Bricks change mid-route | Invalidate and replan | H | `bots.rs:1471-1478`, `:2745-2748`; test `an_objective_recovers_when_a_real_actor_walks_out_of_its_blocked_corridor` | |
| N11 | Crawlspace | Crouch through | H | test `a_bot_crawls_through_a_low_gap` | |
| N12 | Search probes inside walls | Skip to the next probe | H | `search_memory.rs:76-110` | |
| N13 | Leash through portals | Leash follows the way walked | H | `bot_crossed` `bots.rs:2947` | |

### Perception

| # | Edge case | What should happen | Status | Evidence | Smallest fix (folds into) |
|---|---|---|---|---|---|
| P1 | Occlusion | Seen if a fair part of the body shows | P | one eye-to-eye ray, bricks only (`bots.rs:1270`, `simulation.rs:1652-1700`); a head behind a pole means unseen | When the eye ray fails, try a second chest ray; count vehicles as occluders. (Perception) Owner: fix/bot-perception |
| P2 | Enemy behind the bot | Slower notice than one in front | W:fix/bot-perception | `away_scale` (perception lane `bot_kind.rs`); today there is no view cone | Top-10 #5 |
| P3 | Darkness or fog | Sight limited by the map's visibility | N | `kind.sight` (80) regardless | Clamp sight to the environment's visible distance, if the engine exposes it. (Perception) Owner: fix/bot-perception |
| P4 | Invisible or hidden players | Not seen | N *(inferred)* | `bot_sight` checks only alive + ray | Skip peers whose body is hidden, using the same state the client uses to hide them. (Perception) Owner: fix/bot-perception |
| P5 | Very small or scaled targets | Aimed at their real eye | H *(inferred)* | aim at `seen.eye` (`bots.rs:2471`) | |
| P6 | Stale evidence | Expires; never refreshed from hidden truth | H | tests `an_unseen_enemy_keeps_its_observed_position_and_timestamp`, `expired_evidence_stops_searching_without_refresh_from_the_hidden_actor` | |
| P7 | Evidence of a dead player | Dropped | H | test `a_known_dead_subject_invalidates_a_live_search_before_its_expiry` | |
| P8 | Ally evidence (design item 5) | Teammates' last-seen feeds search | H | alerts set `memory` (`bots.rs:1527-1571`); `pursue` re-observes into `search_memory` (`search_memory.rs:76-83`); test `a_bot_that_sees_an_enemy_warns_its_side`. Human teammates do not share; acceptable | |
| P9 | Blasts, sounds, gaze | Glance when idle; capped | W:fix/bot-perception | `perception.rs` (`MAX_STIMULI = 32`, idle-only glances) | |
| P10 | Hurt from behind | Turn on the attacker | H | hurt threat `bots.rs:1286-1298` | |

### Feedback loops

| # | Edge case | What should happen | Status | Evidence | Smallest fix (folds into) |
|---|---|---|---|---|---|
| F1 | Everyone locked goofing | Impossible: goofs are rare, short and end on any enemy, threat or objective | W:fix/bot-surprise + fix/bot-team-roles | interrupt cooldown and end rules (`surprise.rs` `interrupt`); `mood_cap` (`team.rs` `mood`) | |
| F2 | Everyone locked serious | Some goofing even in matches | N (see O15) | | O15. Counted once, under O15 |
| F3 | Oscillation too fast | A held choice with a margin | P | per-pair patches (part 6); commit only at strength > 0 | Top-10 #7. (Hold rule) Owner: fix/bot-surprise |
| F4 | All bots copying one human | Some pull, capped; play also pulls | P | `mood_human` 3, cap 2 (`team.rs` `mood`); a human counts only when they emote (`team_mood`), never as "playing" | Count a human's visible play (moving, firing, on the ball) as a pull toward play; cap per source. (Chooser mood term) Owner: fix/bot-team-roles |
| F5 | Pile-on | Targets spread across enemies | P | nearest enemy (`bots.rs:1313`); overlap only demotes the behaviour | Top-10 #4. (Options + Chooser) Owner: fix/bot-team-roles |
| F6 | Deadlock between two holders | First holder keeps it; the other covers or gives way | H | claims first-come (`claims.rs` `acquire`); cover (`contest.rs:134`); intents tie-break by (since, id) (`team.rs` `terms`) | |
| F7 | Hold rule blocking a needed reaction | Real damage breaks a hold | P | surprise `urgent` only for hurt within 8 m in the last 1.5 s, or health below 0.3 (`surprise.rs` `surprise_gate`) | Any real damage is an interrupt, as perception's header already says. (Hold rule) Owner: fix/bot-surprise |
| F8 | Tell freezing a bot in a fight | Never while urgent | W:fix/bot-surprise | gate clears tells (`surprise.rs` `pick`) | Fold tell into the perception reaction (part 10) |
| F9 | Harm-volume ping-pong (two allies each in the other's line) | Only one steps aside | P *(inferred)* | `harm` applies to both (`team.rs` `terms`); Wander exits (`team.rs` `exit`) | Apply the same (since, id) rule as overlap: the later one moves. (Chooser terms) Owner: fix/bot-team-roles |
| F10 | Callout spam | Rate-limited | W:fix/bot-team-roles | `callout_seconds` | |

### Performance

| # | Edge case | What should happen | Status | Evidence | Smallest fix (folds into) |
|---|---|---|---|---|---|
| Pf1 | Many bots × many options | Shared budgets | H | objective 1 per tick (`objectives.rs:1795-1801`); 32 object sights per tick (`interactions.rs:158`); combat budget (`combat.rs:40-115`) | |
| Pf2 | Sight rays | Budgeted | P | up to N−1 rays per bot per tick (`bots.rs:1301-1316`); with portals every peer is a candidate | Put sight under the shared combat budget with a reserve for the current target. (Perception) Owner: fix/bot-perception |
| Pf3 | New lane rays (mood `clear_between`, surprise `surprise_pause` ray per peer, team sightline, perception watcher polls, surprise surface queries) | Same budget | P | none of these share a budget (`team.rs` `team_mood`; `surprise.rs` `surprise_pause`; `perception.rs` `bot_watcher`) | Same shared budget. (Perception) Owners: each lane |
| Pf4 | Carry `open_spot` | Spread over ticks | P | up to 48 spots × ~17 rays in one tick (`bots.rs:1349-1385`) | Simplify per part 25. (Planner) Owner: fix/bot-route-planner |
| Pf5 | Arming checks every item in 24 m each tick | Budgeted | P | one sight per item (`arming.rs:46-85`) | Shared budget. (Perception) Owner: fix/bot-perception |
| Pf6 | Claims growth | Bounded | H | `MAX_CLAIMS` 16, `MAX_FAILURES` 64 (`claims.rs:5-10`); intents pruned (team lane) | |
| Pf7 | Surprise memory growth | Bounded | W:fix/bot-surprise | one drive per (domain, option); shots drain when due | |

### Determinism

| # | Edge case | What should happen | Status | Evidence | Smallest fix (folds into) |
|---|---|---|---|---|---|
| D1 | Seeded RNG | Same seed, same play | H | `bots.rs:345`; surprise `Mind::new`; tests `the_chooser_is_deterministic_for_a_seed`, `delays_vary_by_seed` (perception) | Merge the three RNGs (part 27) |
| D2 | Seeds and owner ids depend on join order | Same save + same join order replays | H | owner-id seeds | |
| D3 | Tick order fairness | No bot always first | H | `bots.rs:1512-1517` | |
| D4 | Network / client authority | Server-only brains, no traffic | H | `bots.rs:8-10` | |

(The per-round seed reset is counted under L4.)

### Abuse

| # | Edge case | What should happen | Status | Evidence | Smallest fix (folds into) |
|---|---|---|---|---|---|
| A1 | Griefer hurts bots outside a mini-game | Bots stay harmless | H | `bots.rs:15-16`, `can_damage_player` in `bot_enemy` | |
| A2 | Griefer kites a bot away | Leash, then return | H | `bots.rs:2045-2051`, Return | |
| A3 | Griefer boxes a bot in | Give up the goal, wander | H | `bots.rs:2720-2737` | |
| A4 | Griefer holds or stands on the ball | Respect the grip; contest it physically | H | `interactions.rs:375-380`; `physical_objectives.rs:684-686` | |
| A5 | Griefer takes a bot's seat | Human wins | H | V3 | |
| A6 | Bot stuck in a goof loop | Cooldown; ends on any enemy or objective | W:fix/bot-surprise | `surprise.rs` `interrupt` | |
| A7 | Spawn camping | Bots do not insta-kill at spawn | P | I8 | I8 fix. Owner: fix/bot-perception |
| A8 | Emote spam steering bots' mood | Capped | W:fix/bot-team-roles | `mood_cap` | |

### Odd content

| # | Edge case | What should happen | Status | Evidence | Smallest fix (folds into) |
|---|---|---|---|---|---|
| C1 | Add-On weapons with unusual datablocks | Never invented melee; fall back safely | H | `bots.rs:1411-1461`; tests `absent_projectile_and_unknown_scripts_are_never_invented_melee`, `unfamiliar_direct_splash_charge_and_melee_families_fire_and_hit_through_real_controls` | |
| C2 | Hitscan weapons | Use the ray's range | H | test `a_hitscan_guard_understands_ray_range_and_hits_without_chasing_into_melee` | |
| C3 | Hold tools (gravity gun) | Hold and fling | H | Carry; `BotUse::hold` | |
| C4 | Script-driven mini-games | Typed package objectives, rules bots | H | tests `unfamiliar_package_composes_real_pickup_and_return_with_canonical_winner`, `imported_ctf_bot_takes_the_actual_flag_and_returns_it_for_its_team` | |
| C5 | Unusual seat layouts | Roles from data | H | test `unfamiliar_reordered_seats_form_crews_drive_and_fire_their_actual_weapon` | |
| C6 | Swimmer on land | Package policy (the Shark dies) | H | `bots.rs` `out_of_water_seconds` | |
| C7 | Flying wheeled vehicles | Not used | H | `interactions.rs:315` | |

---

## 3. Simplification: what to remove, merge or keep

Size counts are approximate. Risk is the chance that removing or merging the
piece breaks behaviour or tests.

| Piece (file, size) | Kind | What replaces it / why it can go | Risk of removing |
|---|---|---|---|
| Fly behaviour, `air_chase`, `AirChase`, Fly movement (`bots.rs:1320-1347`, `:2233-2242`, `:2627-2705`, ~140 lines) + ball-games `fly_rise`/`fly_drop`/`fly_give_up_seconds` (3 tunables) | superseded | Route-planner jet legs (`route.rs` `Jets`, `JetLeg`) | Low once the planner lands; test `a_jetting_bot_closes_on_an_enemy_on_a_high_brick_platform` and `a_close_ranged_flyer_escapes_a_low_roof...` must pass on the planner |
| Per-pair hysteresis: band slack (2 tunables), Fight→Chase dwell (1), Return all-the-way, ball-games `pursuing` (~80 lines) | redundant with the hold rule | One hold rule (commit + margin) | Medium: band-edge tests in `behaviour.rs` (`the_band_edge_does_not_flip_flop`) need rewording against the hold rule |
| `reverses()` + `mounted.reverse_degrees`/`reverse_distance` (`interactions.rs:1141`, ~40 lines, 2 tunables) | redundant | `route::gear` | Low-medium: test `a_pursuing_chassis_backs_onto_a_near_target_but_turns_toward_a_far_one` |
| Eight progress watchdogs (stuck/replans, wedged, vehicle_stuck at 360/480/720, approach timeout, arming timeout, probe timeouts, fly give-up; ~150 lines) | redundant | One planner progress monitor per leg + claims lease | Medium: many chaos tests depend on exact give-up times |
| Hurt map + `objective_threat` (~40 lines) | parallel bookkeeping | One evidence list with a "hurt me" tag | Low-medium: threat tests in `bot_physical_objectives` |
| Five ally-volume checks (~200 lines) | duplicate geometry | One helper (point/segment/hull vs ally bodies) | Low |
| Three RNGs | duplicate | One per-bot stream | Low (seeds shift) |
| Three weave/strafe patterns | duplicate | One strafe pattern | Low |
| Interaction seat utilities 0.84/0.88/0.72 (`interactions.rs:318-328`) + team `uses` seat bonus + route `drive_serves` | three scorers for one choice | Chooser `uses` term + planner board leg cost | Medium: crew tests in `bot_interactions.rs` |
| Arming `skipped` list | parallel failure memory | `claims.failures`-style cooldown keyed by item | Low |
| Ball-games `committed`/`gunning` constants | special-case scores | Chooser worth (objective worth includes what it carries) | Low; keep the run-and-gun action (keep: variety) |
| Surprise tell (`tell_seconds`) | duplicate delay | Perception reaction delay | Low |
| Four diagnostic views (`BotThought`, `BotSurpriseView`, `BotTeamView`, `BotNotice`) | parallel bookkeeping | One decision record | Low (headless tests read fields; update them) |
| Perception tunables (21) | low value per tunable | ~12: one reach per source, scale rather than separate combat/relaxed/away numbers | Low |
| Legacy `bot_weapon` | redundant later | tactics capability | **Do not remove yet**: needed for unknown images |

**Keep: variety** (removing any of these would shrink the option space or make
bots more predictable):

- Surprise interrupts (11 flavours), splash aims, flank routes, the drift,
  boredom and effectiveness terms.
- Team overlap, uses, harm and mood terms, and callouts.
- Contest lead, clear, walls and cover.
- Carry/fling, arming, the run-and-gun action.
- Weapon choice in tactics.
- Glance sources in perception.
