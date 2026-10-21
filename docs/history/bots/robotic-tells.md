> **Historical note. Written 2026-10-05 (the file's date in the cloud project folder, `bots/robotic-tells.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Bot tells that read as robotic on camera (v0.2.4 showcase)

Read-only audit, 2026-10-05. Branch `claude/project-thread-pt64ji` at
`e1e2c30c` (fix/bot-surprise and fix/bots-ball-games already merged), with
origin fetched. Lanes were read with `git diff HEAD...origin/<lane>`:
fix/bot-perception (7db31811, WIP), fix/bot-route-planner (cab1f873),
fix/bot-team-roles (244f9f28, WIP). **fix/bot-low-fruit and
fix/bot-tuning-tools are not on origin.**

Out of scope because the movement fix already covers it: constant-rate snap
turning (`turn()`, bots.rs:609, `turn_degrees` 300), metronome strafing and
grid zig-zag paths. Where an item below shares a root cause with those, the
row says so.

All line numbers are in `crates/sim/src/session/bots.rs` unless the row
names another file.

## Top 5

1. **Every bot is the same default Blockhead.** `embody_bot` gives each bot
   its kind's avatar defaults (787-810), and `bot.blockhead` has no `look`.
   FFA, goofing, dance and credits shots show rows of identical clones.
   *Owner: none. It is root code (bot_kind/embody); hand it to low-fruit.*
2. **Idle bots are statues, and players right next to them do not exist for
   them.** The surprise interrupts (look, stare, emote, hop) are built but
   `strength: 0` (bots.json). Sight only returns enemies (1300-1355,
   `bot_enemy` 1231). Perception glances fire only on blasts, sounds, a
   1.5 s stare or fast motion. *Owner: perception (an ambient glance at
   nearby players) plus tuning-tools (turn interrupts on). Partly covered.*
3. **Tanks, jeeps and package weapons fire in unison.** The legacy trigger
   pulse is `tick.is_multiple_of(40)` on the global tick (2978, 2984).
   Mounted bots always take that path, because `choose` returns
   `Unsupported` when the bot is mounted (combat.rs:348). Every bot gunner
   fires on the same tick, 3 times a second. *Owner: none (bots.rs
   executor). Not covered.*
4. **Rocket fights look aimbot-like.** Aim goes to the exact chest point
   `seen.eye - 0.5` (2662, combat.rs:363). The lead is a perfect intercept
   on the target's true velocity (combat.rs:361-362, 451-470). The error is
   one uniform offset redrawn every 0.4 s that shrinks to ±1.7° after 2 s
   (2668-2678). Fire goes out at the exact image cycle with no pauses
   (combat.rs:1123-1130). The feet and surface aims exist only behind
   surprise, which is off. *Owner: combat chooser (none) plus perception
   (wobble). Partly covered: perception only varies the first reaction.*
5. **Allies snap to the same spot on the same tick.** Sightings are
   broadcast on global `tick % 30` (2096). `hear_alerts` writes memory into
   every ally in reach in that same pass (1573-1617), with no delay, so a
   squad turns and searches as one. *Owner: perception (fits its reaction
   model). Not covered: its `react_to` covers seen and hurt, not heard.*

## All tells, by severity

| # | Tell: what a viewer sees | Evidence | Severity | Smallest fix (part) | Owner | Lane covers it? |
|---|---|---|---|---|---|---|
| 1 | **Clone army.** Every bot of a kind wears the identical default avatar: same colours, face and decal. Team uniforms hide it only in team modes, so FFA, goofing, dance and credits all show clones. | bots.rs:787-810 (`embody_bot`: `avatar_catalog.defaults` plus the kind's optional `look`); bots.json has no `look` for `bot.blockhead` | High | In `embody_bot`, when a kind has no explicit look (or `look.vary`), draw shirt, pants and accent colours from a small palette, plus a face and decal from the catalog, with the bot's seeded RNG. Keep it per life or per bot. About 20 lines plus one bot_kind flag. | Root (bot_kind.rs / bots.rs core). No lane. Candidate for low-fruit. | No |
| 2 | **Statues when idle.** Between wander legs (a new point every 2-6 s, 490-515) a bot stands dead still facing its last walking direction. It never looks round, and its pitch sits at exactly 0 (2633 `aim_pitch = 0.0`). The built idle flavours (look, stare, hop, emote, crouch, tool) never run. | bots.json `surprise.strength: 0`; surprise.rs:578 (interrupts gated on strength); bots.rs:2610-2622 (`natural` pause) | High | Turn on only the interrupt part: a separate `interrupt_strength`, or a strength of about 0.3 for this kind, after tuning. As a minimum, add a slow seeded look-round to Wander once settled: pick a yaw within ±70° every 1.5-4 s plus a small pitch. | Tuning-tools (strength); perception (glance) | Partly: perception adds stimulus glances for Wander/Return only; surprise has the code but it is off |
| 3 | **Ignores a player standing right there.** Outside a fight, a non-enemy player (sandbox, goofing, credits, a teammate) is never looked at. A bot walks past or stands nose to nose with the camera holder without reacting. | bots.rs:1300-1355 (`bot_sight` returns enemies only via `bot_enemy`, 1231-1253); perception lane `bot_watcher` needs a 1.5 s stare within 6° | High | In perception, make "a player within ~6 m in plain view" a low-salience glance source that does not need their gaze. Cap it with the existing cooldown so the bot does not stare forever. | Perception | Partly (stare and gaze only) |
| 4 | **Synchronized gunners.** Every bot on a legacy-path weapon (any mounted gun, tank cannon, package or script weapon) taps its trigger on the same global tick, every 40 ticks. Two tanks or jeep gunners fire in perfect unison, metronome-regular. | bots.rs:2978 `pulse = fire && ... tick.is_multiple_of(40)`; 2984 (charged vehicle guns); combat.rs:348 (mounted gives `Unsupported`, so always the legacy path) | High | Give each brain its own refire phase: `next_pulse` drawn from its RNG (e.g. 30-60 ticks), replacing `tick % 40`. That is a 3-line change in the executor. | None (bots.rs executor). Movement or low-fruit could take it. | No |
| 5 | **Aimbot rockets.** Rockets go to the exact chest centre with a perfect intercept on the target's current velocity. Error is a fixed offset that jumps every 0.4 s (a visible micro-snap) and tightens to ±1.7° after 2 s of tracking. Bots never aim at the feet, so rockets fly dead into the torso. | bots.rs:2662 (`seen.eye - Y*0.5`), 2664-2667 (exact lead), 2668-2678 (`ERROR_TICKS` 48 redraw, shrink to 1/3); combat.rs:361-363, 451-470 (intercept), 576-589 (feet and surface aims only with surprise on) | High | (a) Replace the step redraw with a smoothly drifting error: lerp toward a new random offset over the 48 ticks, and keep the floor at about half of `aim_error_degrees`. (b) For splash weapons in the plain pick, aim at feet plus 0.15 (the existing `AIM_FEET` variant) by default or 50/50 from the RNG. (c) Lead with a velocity smoothed over ~0.25 s, not the instantaneous one. | Combat chooser: none. Perception lane already multiplies the error with `wobble`. | Partly: perception varies only the first reaction and wobble |
| 6 | **Squad snaps together.** One bot sees you, and on that same tick every ally within 80 units turns to your position and heads there. Several bots pivot as one, then clump at the same last-known spot. | bots.rs:2096 (warn on global `tick % 30`), 1573-1617 (`hear_alerts`: same-tick memory write, same point for all) | High | In `hear_alerts`, store a per-ally `heard_at = tick + rng(0.25-1.0 s)` and apply the memory only after that. Offset the search point by a seeded 1-3 m. | Perception | No (`react_to` covers seen and hurt only) |
| 7 | **Machine fire cadence.** Hand weapons fire at exactly the image cycle for as long as the target is in sight and clear: no bursts, no pauses, no hesitation after a miss. | combat.rs:1110-1130 (`trigger`: down whenever `!last_down && current.down`); bots.rs:2655-2690 | Med | In `trigger` (or its caller), add seeded burst and pause: after N shots (2-5 from the RNG), hold the trigger up 0.2-0.6 s. For semi-auto, add 0-6 ticks of jitter. | Combat executor: none | No |
| 8 | **Identical robot names.** Brick bots are "Blockhead Bot (Blue)", "Blockhead Bot (Blue) 2", and so on, on nameplates and the Players list in the soccer and credits shots. The kind's 46 `first_names` are only used by Slayer rules bots ("Bot Alex"). | bots.rs:1029-1054 (`brick_bot_name`); session.rs:1303-1324 (" 2", " 3" suffix); bots.json `first_names` | Med | In `brick_bot_name`, when the brick has no name, use a seeded first name from `kind.first_names` that no peer already uses, e.g. "Casey (Blue)". The team suffix stays so the Players list still tells them apart. | Root (bots.rs naming); low-fruit candidate | No |
| 9 | **Weapon swap flicker.** The held weapon's candidacy is hard-gated each turn: `near`/`reach` (combat.rs:450), `safe_blast`, `clear_path`. When a rocket target closes inside blast range, or an ally crosses the line, the held slot drops out and the bot swaps instantly, then swaps back next turn. The 0.15 hysteresis only applies when both slots stay eligible. | combat.rs:450, 500-516, 603; bots.rs:3093-3104 (instant `equip_tool` on the same tick) | Med (inferred, not measured) | In `choose`, keep a `switched_at` per brain and refuse a non-forced switch within ~0.8 s of the last one. A held slot that is only transiently blocked stays selected (hold fire instead). | Combat chooser: none (team-roles also edits combat.rs, so coordinate) | No |
| 10 | **Vehicle stop-start.** The driver uses bang-bang throttle: forward 0.25-0.8 or nothing, and handbrake on whenever it coasts (`jump = forward == 0`). Over corner or arrival speed it brakes, then accelerates again, so jeeps and tanks lurch with brake lights pulsing and never use top speed (cruise 0.6). | interactions.rs:1043-1051 (HEAD); route-planner keeps this: its interactions.rs:1060-1070 with `route::CRUISE = 0.6` | Med | Make the throttle proportional, `forward = clamp((target - speed) * k, 0, 1)`. Brake (`jump`) only when speed > target × 1.2, not on every coast. Vary cruise per bot (0.6-0.85) from the RNG. | Route planner (it owns the drive leg now) | No |
| 11 | **Simultaneous dismount.** Every wandering bot in a vehicle gets out on the same global tick (every 3 s). | interactions.rs:1132 `behaviour == Wander && tick.is_multiple_of(CREW_WAIT)` | Med (visible whenever two or more vehicles idle) | Use `tick >= vehicle_since + CREW_WAIT + rng` per brain instead of the global multiple. | Route planner (touches interactions.rs) | No |
| 12 | **Instant respawn in lockstep.** Rules bots (Slayer TDM) respawn on the first tick the game allows (wait 0). Bots killed by one rocket pop back together and sprint out on the same tick. Brick bots all wait exactly 1.000 s. | bots.rs:1648-1661 (`wait = 0` / `120`) | Med | Add a seeded 0.4-2.5 s to `wait` per death (draw when it dies). | None (bots.rs core); low-fruit candidate | No |
| 13 | **Melee bots plant and chop.** A melee bot walks straight in, stops at exactly its band, stands still and swings at max rate while tracking perfectly. There is no lunge, back-step or sidestep. | bots.rs:2761-2768 (melee branch: only `forward` when `gap > near+0.5`, else zero) | Med | In the melee Fight branch, step forward on the swing tick and back 0.3-0.6 s later, with a seeded lateral component. | Movement fix (executors) if it extends to melee; otherwise none | Possibly, if the movement fix covers melee |
| 14 | **Planted in rocket fights.** In ranged Fight a bot never jumps or hops (only when stuck), so it does not dodge rockets the way players do. | bots.rs:2770-2815 (no jump in the ranged Fight branch); 2929 (jumps only when stuck) | Med (inferred) | In the ranged strafe branch, add a seeded chance to jump at a strafe-leg change, or when a hostile projectile is within ~6 m (the perception `Blast` and sound stimuli already exist). | Movement fix (executors) | Possibly, as part of the strafe rework |
| 15 | **Ball-locked eyes and glued dribble (soccer).** At an objective the bot's look is locked exactly on `objective.aim` every tick. It pushes from directly behind the ball along the goal heading, and the in-band weave flips on global `tick/90` parity, so all same-parity bots weave in phase. At kickoff all four sprint to the ball at once (objective turns are staggered 1 tick apart). | bots.rs:2711-2717 (aim = objective.aim), 2748-2759 (`(tick/90 + bot)` weave), 1535-1555 (`objective_candidate` one per tick); progress 2026-10-04 metrics: wrong way 7.5%, own goals ~9% | Med | Weave is covered by the movement fix. Separately, look at the ball with the smoothed or drifting error from #5. Give the second bot on a side a kickoff hold through the team-roles overlap term, so only one rushes. | Team roles (kickoff and overlap), movement (weave) | Partly: team-roles overlap (WIP, weights 0) |
| 16 | **Same reaction time for everyone.** Every bot first fires exactly 0.35 s after first sight, so bots that see a player on the same tick open fire on the same tick. | bots.rs:2682-2686 (`reaction_seconds` constant) | Med | Already built: perception `react_to` with `jitter` 0.3, alertness scales and wobble. | Perception | **Yes** (fix/bot-perception, WIP) |
| 17 | **Eyes in the back of the head.** Sight has no field of view. A bot spots someone directly behind it and turns to shoot them, so sneaking up never works. | bots.rs:1300-1355 (only an LOS ray via `simulation.sight`) | Med | Perception's `away_scale` 1.5 lengthens the reaction. A rear cone (>110°) outside ~4 m could instead need a sound or hurt stimulus before it counts as seen. | Perception | Partly (slower reaction only) |
| 18 | **Rhythmic stuck-hop.** A bot pressing against a wall, a vehicle or another bot hops exactly every 40 ticks (5 ticks of jump in each 40). Two bots stuck on each other bunny-hop in sync. | bots.rs:2929 `brain.stuck > 20 && brain.stuck % 40 < 5` | Low | Draw the next hop delay from the RNG (25-60 ticks), or hop once and then sidestep. | Route planner (owns the stuck and replan path) | No |
| 19 | **Same life replayed.** A brick bot's brain is rebuilt with the same seed after every brick respawn or reset, so its wander picks and aim errors repeat round after round. Most visible in soccer resets. | bots.rs:376 (seed from the bot id only), 771 (`Brain::new` in `respawn_brick_bot`) | Low | Mix `crossed` or the tick into the seed in `Brain::new`, or carry the old `rng` over as `sequence` already is (774-775). | None (bots.rs core) | No |
| 20 | **Callout repetition (if enabled).** Team-roles adds team-chat callouts, one fixed template per term ("Moving!") with a 10 s cooldown. Turned on, the same line would repeat verbatim. | origin/fix/bot-team-roles: team.rs:149-164, 306-315; bot_kind/team.rs:31-33 | Low (off by default) | Allow a list of lines per term, pick one with the RNG without repeating the last, and add a 0.3-1.5 s typing delay. | Team roles | It is the lane's own WIP |

## Notes

- Things checked that look fine on camera: per-bot RNG seeds differ (376,
  `surprise::Mind::new(bot)`), so different bots do not mirror each other
  through shared seeds. The wander timing is seeded (514). Weapon choice has
  0.15 hysteresis when both slots stay eligible. The driving steering is
  rate-limited (0.04 rad/tick) and smooth. Bots never chat. That matches
  v20 bots and is not a tell.
- Several tells share one root cause: decisions keyed to the **global**
  tick (`tick % 40` fire, `tick % 30` alerts, `tick % 360` dismount,
  `tick/90` weave). Swapping each for a per-brain seeded deadline is the
  cheapest single sweep and removes most on-camera synchrony.
- Surprise (merged, `strength: 0`) already holds the look, stare, emote,
  hop, feet-aim and flank variety that tells 2, 5 and 15 need. Enabling it,
  even only for interrupts and the aim domain, is the largest single win,
  but it is a tuning call for the tuning-tools lane, which is not yet
  pushed.
