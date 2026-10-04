# 2026-10-04 One team model for events, bots, the MiniGame window and saves

Max (v0.2.3): Team checks in Wrench events only worked once a mini-game was
running (his goal row's Team menu listed only "No team"), and bots put on
teams were all back on one team after Save & Reset.

## Causes
- **The first team was team 0, which is also "No team".** `Teams::next`
  started at 0 (`crates/minigames/src/model.rs`, `teams.rs` `set_teams`),
  and a Team condition reads 0 for a player on no team
  (`session/rules.rs` `rule_query`), as the wrench's "No team" item does.
  Picking the first team saved 0: it reopened as "No team", teamless
  players and bots satisfied "Team = first team", and the shipped team
  door/Soccer recipes credited teamless movers. Ids also only ever grew, so
  re-added teams soon passed `setTeam`'s 1-32 range.
- **The wrench listed the editor's mini-game, not the brick's.** All three
  team menus (`screens/wrench.rs`) read `core.minigames.active_game`, the
  game of whoever has the wrench open. A brick's rows run in its builder's
  mini-game (v20's brick events act in the brick owner's game), so outside
  a game the menu offered only "No team".
- **A reset made every spawn-brick bot again.** `MiniGameSO::Reset`'s
  `spawnVehicle(0)` reaches `respawn_vehicle_brick`, which dropped the bot
  and joined a new player (`session/vehicles.rs`), losing its team (and any
  Add-On's auto-sort then picked again).
- **Saved builds dropped team identity.** `SavedTeam` had no id and the
  restore made every team new (`id: None`), so a loaded build's Team rows
  could name the wrong team; both saving and the window's Save also refused
  teams unless an Add-On declared a team-scoped setting, contrary to the
  native team editor the Rule Workshop design promises.
- **The MiniGame window lost clicks while anyone scored.** Its refresh key
  was the listing revision, which every score change bumps; the rebuild
  replaced the row under a held mouse button and `View::detach` left the
  press on the old node, so the release missed.

## Fix
- Team ids are slots 1..=`MAX_TEAMS` (64): a new team takes the lowest free
  slot, 0 is never a team (`TeamId::valid`), and a spec naming a slot the
  game lacks creates that slot's team. `setTeam` accepts 1-64.
- `OpenEvents` carries the brick's builder; the wrench's one `rule_teams`
  lists the builder's game's teams for Team checks and `setTeam`.
- `respawn_brick_bot`: the same bot player gets a fresh brain and a new life
  at its brick; only a missing bot or a changed kind is made again.
- Saved builds keep each team's id and restore it in place
  (`edit_settings(.., saved_ids)`); the Add-On team-setting gate is gone.
- The window's refresh key leaves scores out, and `View` hands a press or
  open dropdown on a detached control to the control a rebuild adds under
  the same name.

## Tests (each failed on origin/main 53f05cf, passes here)
Run against main's `src` with these tests in place, then on this branch:
- `bri-minigames --test teams` `team_ids_are_slots_from_one_that_never_read_as_no_team`:
  main fails at teams.rs:377, `left: (TeamId(0), TeamId(1))`.
- `bri-chaos --test bot_soccer_teams` (a soccer pitch: Steel Ball, a goal
  per team, one bot spawner per side, all set up through ordinary commands):
  - `team_sides_and_goal_guards_survive_save_and_reset`: main fails with
    `0 is the No team value of a Team condition: [("Blue", 0), ("Red", 1)]`.
  - `a_saved_build_brings_its_teams_back_in_the_slots_its_rules_name`: main
    fails with `No running Add-On uses teams`.
  - `a_bot_scores_in_its_own_teams_goal_after_save_and_reset`: on main no
    bot scores (`no grounded objective plan`). Here the Red bot plans the
    south goal and wins there.
- `bri-ui --test minigame_screens`:
  - `a_score_changing_mid_click_does_not_rebuild_the_addon_settings_rows`
  - `a_rebuild_between_press_and_release_keeps_the_click_on_the_rebuilt_row`
  - Both fail on main at minigame_screens.rs:1116, `the click landed on the
    row the update rebuilt`, `left: None`.
- `bri-ui --lib` `team_checks_offer_the_builders_mini_game_teams` (in
  `screens/wrench.rs`): the builder is in a game, but the editor is not. It
  cannot compile on main because `OpenEvents` has no builder there.

Commands, each with `CARGO_TARGET_DIR=/home/claude/bri-target
CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=3`:
- The tests above, as `cargo test -p <crate> --test <file>`.
- `cargo test` of bri-minigames, bri-events, bri-sim, bri-ui and bri-chaos,
  plus the bri-addon-import integration tests. All pass apart from tests
  that need an offscreen GPU adapter, which this container lacks.
- `cargo clippy -p bri-minigames -p bri-events -p bri-sim -p bri-ui -p
  bri-client -p bri-chaos --all-targets -- -D warnings`.
- `rustfmt --edition 2024` on the changed files.

The target dir is shared with other worktrees, and a path crate's artifact
hash leaves out the checkout path, so worktrees overwrite each other's
`bri-*` artifacts. The before/after runs above therefore pass `cargo
--config <file>` with a `[profile.dev.package.<crate>] codegen-units = 31`
override for every workspace crate, which hashes this tree's crates apart.
Each run also counted only when cargo had recompiled this worktree's crates
in that same invocation.

## Left
- **A brick bot follows its builder's current mini-game.** That is the same
  rule the brick's events follow (v20).
- **The bot spawn brick has no Team field.** Assignments now survive resets,
  but a build load or Change Map makes new bots with no team. Fixing that
  needs a world/save schema field on the brick.
- **Only one bot plays the ball, for the bot lane.** The ball is an
  exclusively claimed objective resource. The other side's bot reports
  `objective resource claimed` and does something else instead of
  contesting the ball.
- **The shipped "Ball goals" recipe is not playable by bots, for the bot
  lane.** Its Team Score guard has no fact key (`objectives.rs:682`), and
  `resetObject` is in `objectives.rs:802`.
- **The wrench lists only "No team" outside any mini-game.** This is still
  true, and correct: the rows run in the builder's game, so a Team check
  needs the builder in a game that has teams.
