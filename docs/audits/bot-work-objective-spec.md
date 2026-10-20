# Spec: a general "work a place" objective for the shared bot brain

Why: a team game's objective is often *stand at a place and hold a control until the rules say it is done* (America's Army valves and
control panel, a capture point, a hacking terminal, a button that must be held). The shared brain has `carry_return` offers (pick up,
deliver, a counter rises) but nothing for this, so the AA add-on carries its own bot logic: an objective picked by `bot id % 4`, offline
route lists and a door-opening script. Max's rule: no per-game cases in the brain. The AA side must only *declare*; the brain plans, the
chooser scores, claims spread soldiers.

## 1. New offer kind `work` (returned by `bot_objectives(p)` like `carry_return`)

```rhai
#{ kind: "work", id: "valve:primary",
   at: [x, y, z],                 // where to stand (feet), or source: #{kind: "brick"|"mover"|"actor", ...} the planner resolves
   radius: 1.5,                   // how close counts as at it
   command: #{ package: "aa25-soldier-showcase", name: "aause", hold: true },   // the rules command a player's key sends, held like the key
   seconds: 10.0,                 // how long the rules need it held (used to time out and to score: longer is less attractive)
   open: #{ scope: "global", key: "hud_primary_on", path: [] },   // workable while this declared state is truthy; gone when it falls
   completion: #{ scope: "player", key: "ob_done", path: [] },    // a declared counter that must rise for the plan to count as done
   claimants: 1,                  // most bots working it at once (default 1)
   weight: 1.0 }                  // the rules' priority (required before optional, instant-win before the rest)
```

Execution (ordinary controls only, like every step): the route planner walks to within `radius` (the existing `Mode` legs incl. ladders and
doors); the bot stands still, faces `at`, and holds the named command exactly as the player's bind does (the same command path the Use key
takes, so rules need no bot branch); it releases on completion, on `open` falling, on damage/enemy-in-sight (the usual interrupts), or after
`seconds` plus a margin without the counter rising (failure/cool-down as today). A fight outranks it unless the bot is committed (the existing
`View::committed` rule): holding for the last 20% of `seconds` counts as committed.

Scoring: utility = `weight` x (1 / (1 + path cost)) x (1 / (1 + seconds/10)), through the same `behaviour::scores` terms as other objectives,
then the existing hold rule (10% hysteresis). Nothing about valves or AA appears in the brain.

## 2. Claims and spread

`claims.rs` already stops two bots taking one body. Extend it to offers: at most `claimants` bots hold a `work` claim; a refused bot scores
the next offer or `guard` (below). Together with the per-life route `veer` (`route::Costs::veer`) and the per-bot timing seeds already in the
brain, soldiers sharing an objective take different routes and leave at different moments. No spread code in any add-on.

## 3. New offer kind `guard`

For the side that does not progress the place: `#{ kind: "guard", id, at, radius, claimants, weight }`: stand within `radius`, face the likely
approach (the brain's existing `spots`/sightline choice of where to stand), fight what comes (the Fight behaviour already wins on sight).
Utility is lower than `work` of the same weight. Per-bot queries already let rules offer `work` to the attacking team and `guard` to the
defending one.

## 4. Doors: `bots_open` names the command

`bots_open` on a mover is today a flag and a rules script presses the door. Make its value the command to send: `bots_open: "aa25-...:aause"`.
When the route crosses such a mover shut, the bot stops in reach facing it and sends that command once (the same call a player's Use key makes
with the mover in reach, `movers_near`), then goes on when it is open. This is the planner's own step ("open this mover"), costed in routes.

## 5. Removed from the AA add-on when this lands

`id % 4` choice (`sm_goal_of`), the offline route lists (`pipeline_routes.json`, `recover_pipeline_routes.py`), `sm_walk_route`, `sm_bot_doors`.
The add-on keeps only the declarations: its objectives as `work`/`guard` offers, its doors as `bots_open` movers.

## 6. Acceptance (a live run, not a test suite)

At round start, soldiers with the same objective leave their spawn at different moments and use different routes (compare positions at
10/20/30 s); each attacker works an open objective until its percentage completes; a refused claimant takes another objective or guards;
defenders guard; doors open without any AA script. Unsupported offers are named in Explain ("no grounded objective plan"), never assumed.
