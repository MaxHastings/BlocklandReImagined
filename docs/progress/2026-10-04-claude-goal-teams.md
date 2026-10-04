# 2026-10-04 Goal teams: a goal scores for a team, one team score

Branch `fix/goal-teams` (from `claude/project-thread-pt64ji`). Max (v0.2.3)
built a soccer goal in the Rule Workshop and could not pick Team 1 or Team 2
in its IF; his own goals scored for whoever touched the ball last; a loaded
build's goal ignored his ball; Slayer's `IncScore` points never reached an
IF Team Score.

## Causes (file:line on the branch base f24065a)
- **No team to pick.** `rule_teams` (`crates/ui/src/screens/wrench.rs:813`)
  listed only the teams of a game the builder was a member of: a builder who
  built the goals first, or owned the game without being listed as a member,
  saw only "No team". "Who/what: Team" had no way to name a team at all:
  the Team number check is offered only for Player/Instigator
  (`wrench.rs:795`), a condition's `key` was for variables only, and IF Team
  Score always read the instigator's team (`crates/sim/src/session/rules.rs:184`).
- **Own goals.** onObjectEnter's Instigator is whoever last moved the ball
  (`mover_credit`, 5 s, or its driver: `rules.rs:812`). Player
  `addTeamScore` (`rules.rs:535`) adds to that player's own score, so a
  defender's own goal scored for the defender's team (the lab recipe hid
  this with an Instigator Team guard, so an own goal scored nothing).
- **Two team scores.** Native Team Score summed members' scores
  (`rules.rs:184-198`); Slayer kept `IncScore` points in its own script
  state (`slayer.rhai` `team_points`), so a goal's Team(Brick) IncScore never
  reached a native IF Team Score.
- **Whose balls a goal sees.** A region saw only objects from spawn bricks
  of the region brick's owner (`rules.rs:851`), and object outputs needed
  the same (`events.rs:1977`, resetObject's own check): an administrator
  editing a loaded build's goal (ownership kept) had a goal that ignored
  their own ball.

## Changes
- **One team score.** `MiniGame::team_points` holds a team's own points;
  `Minigames::team_score` is those plus the members' scores, and
  `event_team_score` sets or adds them (`Effect::TeamScore`, which fires
  onRuleScoreChanged). A reset clears them unless scores are kept, as it
  does members' scores. Scripts: `add_team_points` / `set_team_points`
  (op `SetTeamPoints`, `minigame` capability) and a team map's `points`.
  Slayer's `IncScore`, `/teams score`, End of Round report and points to win
  now read the engine's points (its own `team_points` state is gone; the
  points-to-win check after IncScore counts the points it just added).
- **A goal credits a fixed team.** MiniGame `addTeamScore` (Team, Points)
  and MiniGame `winRound` (Team) in the core catalog (`RuleOp::TeamPoints`,
  `RuleOp::TeamWin`). An IF Team check may name a slot in its `key`
  ("1"-"64"; empty is the instigator's team), read by `Condition::team_slot`.
- **Wrench.** `rule_teams` lists the builder's game's teams, else the teams
  of a game they own, else "Team 1".."Team 8"; a saved slot no team has
  shows "Team N (no such team yet)". "Who/what: Team" gets a "Which team"
  menu ("Instigator's team" or a team). setTeam and the MiniGame team
  outputs use the same team menu. On another builder's brick the Named
  brick line says whose names it lists ("Named brick (Bob's bricks):",
  `OpenEvents::builder_name`). An Add-On target may carry a one-line
  `description` (behaviour.json `brick_targets`, `TargetDef::description`),
  shown under a row aimed at it; Slayer's Team(Client) and Team(Brick) have
  one.
- **Whose balls a goal sees.** An object counts when its spawn brick is the
  builder's or belongs to someone who may edit the builder's events
  (`Session::may_edit_events_of`, `Actor::may_edit(group, trust::EVENTS)`,
  the same check `SetEvents` makes; administrators always). Load ownership
  is unchanged; a stranger's ball still does not count.
- **Soccer recipe** (`lab_programs("soccer")`): each goal's rows are
  1. onObjectEnter, IF round running, IF Object Variable scored = 0 ->
     MiniGame addTeamScore [attacking team] 1
  2. onObjectEnter, IF round running, IF Object Variable scored = 0, IF Team
     [attacking team] Score >= 5 -> MiniGame winRound [attacking team]
  3. onObjectEnter -> Self setVariable Object scored 1
  4. onObjectEnter (3000 ms) -> Object resetObject
  plus the lab's Object kind and Spawned by checks on every row.

## Answers
- **Row order.** Rows run in order and each row's IF is read when it runs
  (`crates/events/src/runtime.rs:1156-1158`, `host.query` per job), so row
  2 sees row 1's points in the same firing. Tested in both the sim recipe
  test and the Slayer soccer test.
- **Re-arming.** Reproduced headlessly: a goal fires again for each ball a
  resetObject (0 ms or delayed) brings back to its spawn at rest, for a ball
  that leaves and returns (teleported or rolling), and for the v20 way
  (`<NAMED BRICK>` _ballspawn -> respawnVehicle). No re-arm bug on this
  branch; a different builder's same name does not resolve. Likely causes
  of "never fires again" in Max's build: the ball's spawn brick was not the
  goal builder's (fixed above), or rows targeting Instigator got no
  instigator because the ball rolled in more than 5 s after its last touch.
- **Double score** during the reset delay (ball bounces out and back in) was
  real for the old recipe; the recipe's Object variable stops it.
- **Instigator winRound in a Slayer team game** ends the round for the
  instigator's team (and names the player): an own-goal instigator wins for
  their own team, hence MiniGame winRound [team].

## Tests
- `bri-sim --lib`: `a_goal_fires_again_for_each_ball_its_reset_brings_back`,
  `a_named_spawn_brick_respawn_vehicle_resets_the_ball_and_rearms_the_goal`,
  `a_goal_fires_again_when_the_ball_leaves_and_comes_back`,
  `a_goal_never_respawns_another_builders_named_spawn` (guards, pass on the
  base too); `a_loaded_goal_sees_the_ball_of_an_administrator_who_may_edit_it`,
  `a_goal_sees_a_strangers_ball_only_once_they_may_edit_its_events`,
  `the_soccer_goal_credits_its_attackers_once_per_ball_even_for_an_own_goal`
  (fail on the base). Three older tests now build their Instigator rows
  explicitly (the recipe no longer has one) and one turns the LAN host off
  so its spawner owner is a stranger.
- `bri-addon-import --test slayer`
  `slayer_soccer_goals_score_for_their_colour_and_reset_the_ball`: Red and
  Blue, points 3, goals painted the attackers' colour with Team(Brick)
  IncScore 1 and a 3 s resetObject; a Red own goal scores for Blue, the
  ball returns to its spawn and each goal fires again, an IF Team [Blue]
  Score >= 2 row sees the IncScore points, Blue wins at 3, `/teams score`
  shows the counts, and the next round starts at 0 and nobody wins it again.
- `bri-ui --lib`: `a_goals_team_checks_offer_team_slots_however_the_builder_starts`,
  `minigame_add_team_score_names_its_team_from_the_team_menu`,
  `named_bricks_say_whose_names_they_list_on_another_builders_brick`,
  `an_add_on_target_shows_its_description_when_chosen`.

## For the bots lane
- Objective reading (`crates/sim/src/session/bots/objectives.rs`) needs: a
  MiniGame `addTeamScore` / `winRound` row as an effect on the team in its
  first parameter (not the instigator's), and a Team Score condition with a
  `team_slot()` read against that team's `Minigames::team_score`. Any goal
  scored by any player of the attacking side (or none) achieves it.

## Win loop (Max: a team won at 5, then the game reset and won again forever)
Setup: Slayer Team Deathmatch, Points 5, Clear Scores on Reset, goals with
only onObjectEnter -> Team(Brick) IncScore 1 and Object resetObject.
- **Base mechanism.** On f24065a a team's IncScore points lived in Slayer's
  script state and were cleared only by `start_round` in its `on_minigame`
  reset handler (`slayer.rhai:1129-1138`), after `revive_bricks`,
  `reset_line`, `reset_cps`, `note_kits`, `note_team_lives` and the team
  shuffle. `on_tick`'s `check_points` (`slayer.rhai:2007`) runs every tick
  once `round_over` is false, which the engine reset sets at once. Hook
  results are discarded (`game_hooks.rs:291`, `let _ = run_package`, under
  `Budget::Command`), so if anything before `start_round` failed or ran out
  of budget the points stayed at 5 and the next tick ended the round again,
  each reset. Members' scores were never the problem (the engine reset
  zeroes them itself), and the key format (`${game}:${team}`) matched
  between `add_team_points` and the clear loop.
- **Not reproduced headlessly on the base**: the exact setup (new test
  below) passes against the base `slayer.rhai` + `behaviour.json` too, so
  the failing step in Max's reset handler is specific to his world (most
  likely the handler's budget with his bricks/bots, or a script error in one
  of those steps); his log would show a `script.error` on `on_minigame`.
- **Fix on this branch.** Team points are engine state
  (`MiniGame::team_points`) and the engine's reset itself zeroes them with
  the members' scores (unless `keep_scores`), in the same command that
  clears `round_over`, so no package handler, ordering or budget can carry
  a won score into the next round. A goal re-firing on the reset ball cannot
  re-win either: `inc_team_points` ends a round only while `running`, and a
  point scored while the round is over is cleared by the reset.
- Guards: `bri-minigames --test teams`
  `a_reset_starts_team_points_again_unless_scores_are_kept`;
  `bri-addon-import --test slayer`
  `a_goal_won_round_resets_once_and_the_next_round_starts_at_nothing` (Max's
  exact rows, 0 ms reset: five goals, one "won this round", the next round
  at (0) and still in play 18 s later).

## Teams page in a mode without teams
The Add-On Settings Teams tab was greyed out and the page forced back to
Setup when `teams_shown_when` did not hold, with nothing saying why. The
Teams tab now stays open while there is a game; in a mode without teams it
shows one line built from the Add-On's own rule (the setting's title and the
choices of it that turn teams on), e.g. "This Game Mode doesn't use teams:
set Game Mode on Setup to Team Deathmatch.", and no team rows, team picker,
team settings or Add Team. The Players tab stays off in that mode. Test:
`bri-ui --test minigame_screens`
`the_teams_page_says_how_to_turn_teams_on_in_a_mode_without_them`. Why Max
saw editable team rows in Deathmatch on v0.2.3 is not reproduced: the gate
was already there and the real Slayer metadata test exercises it.

## Left
- A "Team(Brick) here: Red" line needs the rules' own resolver
  (`event_teams`/`control_of` in slayer.rhai) reachable from the wrench: a
  package query hook, not small.
