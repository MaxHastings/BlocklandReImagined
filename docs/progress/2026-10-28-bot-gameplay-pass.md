# 2026-10-28 Bot gameplay pass before NPC audio

Branch: `codex/afghan-tank-navigation`. Maxwell explicitly requested a broader
core gameplay pass after the Afghan tanks, then contextual sound immersion,
then a PR and merge to remote main. On review he correctly challenged the
premature move toward audio. Audio was paused for the gameplay pass, then resumed
at the evidence-based diminishing-returns point described below; merge remains
blocked on the required validation.

## Acceptance and evidence

Keep the saved Afghan match and its rules intact. Observe ordinary controls,
combat, destruction, deaths, respawns and round transitions; do not inject bot
destinations or drive controls. An improvement in maximum tank displacement
does not establish the quality of the whole squad or objective completion.

- [x] Reproduce the spawn-area tank problem and compare actual routes against
  usable street corridors and midfield/enemy-base approaches. See
  [tank evidence](2026-10-28-afghan-tank-travel.md).
- [x] Correct blocked-driver liveness without allowing pedestrian-only gaps.
  The unchanged 23-test `bot_interactions` suite passes. False headway from
  recovery movement had kept an unproductive driver mounted indefinitely;
  headway now requires reducing the gap to its destination. Recovery checks
  and commands the same direction, brakes opposing momentum, and straightens
  the wheels. Real contact and drop checks remain authoritative.
- [x] Trace all bots in the actual match, including goals, travel, perception,
  combat admission, objectives and body lifecycle. Investigate sustained
  failure candidates rather than treating every pause as a bug.
- [x] Resolve supported general gameplay defects and challenge them with
  ordinary integrated scenarios; record retained limitations explicitly.
- [x] Review ten-minute gameplay evidence and pass quick/targeted regressions
  before resuming audio. Full focused verification passes; the full gate
  and Windows PR checks remain required before the eventual merge.

The opt-in `bot_vehicle_routes` test now also samples every bot's physical
state, vitals, perception/task evidence, objective state, pre-seat walking
intent and actual adapted driving controls. `tools/analyze_bot_match.py`
summarizes vehicle travel, lifecycle transitions, fire admission, driver
stopping evidence and sustained stationary travel candidates. Samples are
one second apart; absence of a brief event is not proof it never occurred.
Private reports and saved-build data stay in ignored local `artifacts/`.

## Open investigation

Maxwell clarified the stopping rule: prioritize high-value recurring failures,
then switch to audio at diminishing returns. Acceptance is a ten-minute ordinary
match covering every bot: vehicles and walking, weapon use, combat/avoidance,
route variety and flanking, team spacing versus solo travel, objectives and
explicitly programmed goofs. It is not an unbounded request for every possible
AI feature. Small or unproven residuals should not hold the whole task forever.

The runover capability tested horizontal proximity but not vertical contact
reach. A regression contrasts a grounded opponent with an opponent on a high
roof inside the same horizontal gun safety radius: it failed on the roof
assertion before correction and passes with instantiated collider height versus
the target's scaled posture height. Boarding opportunity, close-runover gun/seat
choice, direct ramming and unarmed transport/dismount use the same predicate.
All five collision/capability unit tests pass. An ordinary brain/seat regression
is also being checked; this is not whole-gameplay acceptance.

The ordinary 24-test interaction suite passes, including the new lone gunner
staying at its useful gun rather than trying to ram an enemy above its hull.
The same missing vertical-contact predicate affected allied traffic avoidance:
a roof/overpass occupant could close the road below. Actual instantiated body
heights now also gate allied player/body corridor checks. Six collision/contact
unit tests pass, including a street ally that still requires braking.

The 180-second all-bot match had zero control-step failures. All 15 bots had
observed shot ticks; 14 had observed body replacements, and no on-foot bot met
the diagnostic's sustained stationary-travel criterion. Those coarse observations
do not establish aim quality or objective wins. The same trace exposed the
south tank still near spawn (6.23 units versus the north tank's 123.55), with
repeated crew turnover despite bounded per-driver recovery. This blocks completion.
The diagnostic now also retains stationary vehicle candidates across changing
drivers, so a succession of individually bounded attempts cannot hide the issue.

On-demand route-start evidence showed clear spawn headroom and valid neighboring
nodes, but a disconnected small route over the raised plate. The wide chassis
straddles two support heights while crossing a step. `Body::clearance` previously
lifted the query only halfway through its step, treating its old support top as
a wall through its rear footprint when sampling the lower floor. Conservative
chassis clearance now admits the same step/slack as the graph's `edge`; real
posed hull sweeps remain authoritative. A source-free low-clearance body leaves
and re-enters a step-height plate without jumping and still rejects a tall wall.
All 41 non-ignored navigation unit tests and the quick tier pass. In the next
180-second match, two south vehicles each advanced about 106 units. The following
600-second candidate completed with zero bot control failures; both sides'
vehicles advanced and many foot bots crossed midfield/enemy approaches.

That longer trace exposed a general objective failure after explosion impulses:
two on-foot bots stood facing distant goals with empty routes for long periods
after landing far from where their airborne search failed. An empty/finished
route was cached without its physical pose. `Brain::path_stop` now invalidates
that conclusion after meaningful displacement without abandoning the objective.
The regression performs a failed search in the air, then a landing, and verifies
that ordinary planning resumes with the same goal. All three navigation-body
tests pass. No force/physics tuning or invented bot displacement was added.

`afghan-full-behaviour.json` is the next ten-minute candidate, with per-tick
projectile observations, complete canonical damage ledger reads (loss is an
error), deaths and actual round outcomes, plus sampled inventories, image state,
tactical choices, team evidence and explicit goof states. Observed projectiles
are not an exhaustive shot counter: an instantaneous hit may disappear within
one step, which is why actual damage and selections are recorded too.
`tools/render_bot_match.py` creates a private offline 1 Hz movement/decision
timeline. Neither diagnostics tool assigns bot goals or gameplay controls.

## Reviewed result and stopping point

`artifacts/afghan-gameplay-acceptance.json` completed 600 simulated seconds
(138.12 s wall time) with all 15 NPCs and zero bot control failures. It includes
the enabled package set and final read-only rule store. The saved owner is a
passive human participant; this is not a prediction of match balance with an
actively playing host. The original saved-build SHA256 is still unchanged.

The actual CTF owner counters, rather than a route/arrival prediction, record
six flag pickups (two by team 1, four by team 2), two own-flag recoveries (one
per team) and one team-2 capture. No round ended: the capture requirement was
not reached. Both spawn cohorts crossed midfield; across real replacements,
maximum observed displacement was 156.19 south and 123.01 north units.

The trace records spear, grenade, sword and tank-gun use, real accepted damage,
direct/left/right approach decisions and both close allied motion and isolated
travel. Coordination terms are observable; geometric following-like motion is
not evidence of an explicit follow order. Goofs are identified by the brain's
actual interrupt state, not inferred from odd-looking motion. One remaining
14-second search/approach delay occurs in difficult geometry while pursuing
dated elevated evidence. It is bounded and is retained as a lower-priority
residual, not labeled a programmed goof or silently counted as solved.

A separate recurring shuffle occurred beside a chassis when each changing grid
waypoint recomputed a different way around it. The local executor now retains
its chosen side while the same body obstructs it and follows that body's
boundary; a clear direction releases the detour. Seven collision/contact tests
pass, including route-point perturbations that must not reverse the detour.
Idle partial drives also finish rather than being promoted to endless pursuit;
meaningful objective/search/chase partial segments still continue.

Two broader-check failures were diagnosed without forcing vehicle use or
relaxing acceptance thresholds. The look-steered pursuit fixture's ordinary
carbine killed its passive target before pursuit could be observed. Only that
invented fixture's direct damage is reduced, before package/mode installation;
its boarding, turning and travel assertions remain intact and pass. An early
attempt to swap weapon packs after installing rules reset the test's minigame
catalog and was removed. The gauntlet's exclusive `vehicle` category also hid
all mounted fighting/objective activity. Activity and mounted time now measure
separate axes; every fight/objective floor is unchanged, entropy does not double
count riding, and the vehicle deathmatch plus three classification tests pass.

The reviewed quick tier passes (462 sim unit tests, five intentionally ignored
long cases). The repeated full focused tier passes: formatting, warning-free
clippy, changed/dependent non-GPU integration tests and client units (719 passed,
78 intentionally ignored long/GPU cases). Logs retain the earlier failures and
corrections. The offline HTML
timeline was generated and its layout inspected with one bounded headless
browser render; no interactive game session was launched or controlled.

The high-value reproduced failures are corrected and another speculative bot
feature would now have less value than the requested audio phase. Audio work
resumes while required validation continues; a new material failure takes
priority over the audio work. Human playtest acceptance and the alpha contract
remain open.

Maxwell performs interactive acceptance. No interactive game window or input
automation is part of this work. The full alpha acceptance remains open.
