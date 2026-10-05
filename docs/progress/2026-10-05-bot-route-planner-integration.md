# 2026-10-05 Bot route planner: integration merge and walk-leg follow-ups

Branch `fix/bot-route-planner`, merging `origin/claude/project-thread-pt64ji`
(merge commit, no rebase). Not yet green: see "Open failures".

## Merge resolutions

- One reversing rule: `route::gear` (with `Driving`, `Chassis`, `pace`).
  The other branch's separate reversing code is dropped.
- The Fly behaviour and the `fly_*` tunables are dropped; a jet leg of the
  route planner is the only way a bot leaves the ground. `Behaviour::COUNT`
  replaces the literal score-array sizes.
- Kept from the other branch: ball-game objectives, objective deadlines,
  the carrier, surprise.
- Stricter-progress fixes: kept, and the planner's own outcome checks now
  cover two of them. A walk leg judges headway by net displacement over a
  window (`route::Progress`), which subsumes "moving but not arriving"; a
  drive leg judges it by the chassis anchor (`VEHICLE_PROGRESS`), unchanged.
  The claim-progress and approach-timeout fixes are kept as they were.

## Walk legs

- String-pulling (`nav::pull`, `Ground::walkable`): a plain walk waypoint
  heads for the farthest later one of the same walk that the full-width
  standing body walks to in a straight line. The check sweeps the body box a
  step up against the map and loose bodies (ball, parked vehicle; not
  players), and samples the floor every half cell. Jump, crawl, opening and
  other-leg waypoints, and the waypoint where each starts, are never
  skipped. A chassis route is not pulled. Walking had no per-waypoint
  easing before this, and has none now.
- Progress monitor (`route::Progress`): over 0.75 s a walker must cover a
  fifth of what its walk speed would carry it. The first stalled window
  hops; the next forces the replan; giving up works as before. A bot at
  work beside its objective, a seat or an arming point is exempt.
- Walkers detour round seated vehicles (`bot_vehicle_detour`), but not
  round seatless loose bodies such as a ball, which they push on purpose.

## Drive and leave legs

- Standoff: a chassis that cannot hurt its target (no gun, and the target
  mounted or no runover) brakes so as to stop with its nose half its length
  plus `LEAVE_REACH` from them. It no longer tailgates or rams them, and
  the bot gets out there only if on foot it would still close on them
  (`route::walk_closes`).
- A runover vehicle is costed at its top speed in `drive_serves`, and only
  when the rules let this bot hurt that enemy (`can_damage_player`) and the
  enemy is on foot. Before this, any jeep "served" any fight, and bots in a
  ball game went for a parked jeep.

## Tests added

- `nav::tests::a_pulled_diagonal_across_open_floor_is_about_a_straight_line`
  checks that the raw grid route is more than 5% longer than straight and
  the pulled route is within 5%.
- `nav::tests::a_pulled_route_still_takes_the_door_and_cuts_no_corner`
- `nav::tests::pulling_keeps_every_jump_and_where_it_starts`
- `route::tests::a_body_wobbling_between_two_spots_is_stalled_within_a_window`
- `route::tests::a_walker_closes_only_on_a_target_drawing_away_slower_than_it_walks`
- `bot_routes`: a `play_on` fixture resets the mini-game after each won
  round, so the soccer tests keep playing for as long as they watch.

## Commands

All runs used `CARGO_TARGET_DIR` outside the worktree,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_INCREMENTAL=0` and `CARGO_BUILD_JOBS=2`.

- `cargo clippy -p bri-sim -p bri-chaos --all-targets -- -D warnings`: clean.
- `cargo test -p bri-sim --lib`: 238 passed, 2 ignored (pre-existing).
- `bot_routes` 9/9, `bot_brain` 18/18, `bot_interactions` 18/18,
  `bot_knowledge` 8/8, `bot_tactics` 13/13.

Gauntlet, one round each:

| Scenario | Plain merge, before these follow-ups | After |
|---|---|---|
| a_jeep_on_each_side | stuck 7.4%, kills 16, mounted 3579 | stuck 8.1%, kills 12, self-kills 3, mounted 7593 |
| water_between_the_sides | stuck 6.8%, kills 23, switches 24.3 | stuck 2.7%, kills 22, switches 27.0 |
| checkpoint_race | circling 12.4% | circling 21.2% |
| zombie_survival | circling 8.4% (failed), kills 48 | circling 3.5%, kills 45 |

## Open failures

None of these is loosened, skipped or ignored.

- `checkpoint_race`: circling is 21.2% against a limit of 17%. String-pulling
  causes it: with pulling off the scenario reads 12.6%, and with a pull reach
  of 2 it reads 13.4%. The track is out-and-back along one line. Without
  pulling, each route first steps to the centre of the cell the bot stands
  in, which keeps the racers in lanes. Pulled routes start straight from
  wherever the bot is, so racers stay offset, sidestep each other and
  double back near the middle checkpoint. Keeping the first waypoint gave
  17.4%, still over the limit.
- `a_jeep_on_each_side`: stuck is 8.1% against 8%, and `mounted_ticks` is
  7593 against 10000.
- `water_between_the_sides`: 27 switches a bot-minute against a limit of 20.
  The fight is real now (22 kills, against 6-15 when the limit was set): the
  switches are chase and fight changes.
- `bot_soccer_teams::recipe_goals_bots_contest_one_ball...`: fails at line
  813 (both sides working one ball, `contested > 120`). Not investigated.
- `bot_soccer_match::other_line_ups...`: 2v2 obstacles seed 2 reads wrong
  way 0.111 against 0.10. The other 19 runs pass, and two-against-two passes
  on every seed.

## Engine findings (not fixed here)

- Mover credit goes to a victim touching a driven vehicle, so runovers can
  count as self-kills.
- Parked jeeps sometimes blow up in physics and fling jeeps and bots off
  the map.
- `runover_damage: 0` in a vehicle definition is read as "use the default
  5", so a fixture's harmless pursuer is not harmless.

## Next

- Fix the open failures above.
- Message C: proportional throttle (handbrake only for real stops and tight
  turns) and a per-bot cruise spread through the cadence helper
  (cherry-pick 37c290a7), with the straight-drive speed-trace property test.
