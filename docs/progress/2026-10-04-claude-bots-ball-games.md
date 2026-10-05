# 2026-10-04 Bots play ball games, contest the ball, and keep pursuing from a vehicle

Branch `fix/bots-ball-games` (built on `fix/teams`, whose team slots 1..=64
and reset-in-place it uses). Three findings from playtest review, plus a
Team choice on the bot spawn brick (B6). Content names are never checked:
everything below works from rule semantics, claims and kind data.

## Finding 1: the "Ball goals" recipe was unplayable by bots

- **Verified.** On origin/main (and on `fix/teams`), two bots on the recipe's
  goals showed "no grounded objective plan", targeted their goal on 0 ticks,
  and chased and fought each other instead.
- **Cause.** `objective_fact_key` returned `None` for a Team Score
  condition, so `winRound`'s `Team Score >= 5` guard could never be met.
  The recipe's delayed `resetObject` on the ball was an unknown output, and
  the physical-action builder dropped unsupported outputs silently.
- **Change** (`crates/sim/src/session/bots/objectives.rs`). Team Score is a
  fact: the team's total, the sum of its members' scores; `addScore` and
  `addTeamScore` raise it. A delayed `resetObject` on the captured Object,
  from a brick whose owner owns the object's spawner, is a known effect
  after the scoring group (a new object incarnation). A step expecting it
  does not fail when the object vanishes, and replans at once. Any other
  unknown output now marks the plan unsupported.

## Finding 2: no contest

- **Verified.** With only the Team Score fix, the second bot stood down
  with "objective resource claimed": the claim on the one ball was
  exclusive for everyone, and the lease lapsed into a cooldown.
- **Cause.** `claims.rs` treated every claim as exclusive. The lease was
  renewed only by physical progress the waiting bot could not make.
- **Change.** Claims on a loose body conflict only between allies; seats
  stay exclusive (`Resource::contends`). The new `bots/contest.rs` piece:
  when an opponent is the body's mover or holds a live claim on it, the
  approach leads the body along its velocity, and being within `engage` of
  it counts as progress. A step waiting on an admitted delivery holds no
  lease. Tunables in `bots.json`: `contest` (`lead_seconds` 0.6, `max_lead`
  4, `engage` 4) and `objective_radius` 24, which replaces the physical
  provider's `DISCOVER` constant.
- **Fight vs Objective.** These bots are unarmed with an objective, so
  `objective_without_attack` / `peaceful_objective` already keep Fight
  (0.8) from overriding Objective (0.65): they do not fight. The new test
  bounds fight/chase ticks to at most 10% of play. Armed bots still prefer
  Fight by design. The kind's `fight` weight controls that.

## Finding 3: a tank driver retreated

- **Verified.** A bot from a brick at the origin boards a vehicle about 40 m
  out while a human about 60 m out retreats. On origin/main the bot failed
  at tick 111: behaviour "return", goal (0.25, 0.4, 0.25), its brick.
- **Cause.** The driver's leash was measured from home with the on-foot
  `chase_radius` 48. Separately, `bot_seated_input` reversed whenever the
  goal was more than 1.8 rad off the hull, however far away it was.
- **Change.** An explicit per-kind mounted pursuit policy, `mounted` in
  `bots.json`: `anchor` `"mount"` (the leash is measured from where it took
  the controls) or `"home"`, `chase_radius` 96, `reverse_degrees` 103,
  `reverse_distance` 16. While pursuing (Fight/Chase/Search/Fly) a chassis
  reverses only onto a target within `reverse_distance` and turns toward a
  farther one. Fixed deliveries and Return still reverse (limiting those
  broke `repeated_object_entry` and `unfamiliar_ground_vehicle`).

## B6: Team on the bot spawn brick

- `bri_world::VehicleSpawn::team: Option<u32>` (serde default, validated
  1..=64; `WORLD_SCHEMA` unchanged, since an absent field reads as none), and
  `WrenchProperties::vehicle_team`. `setVehicle` keeps it, and blueprints
  copy it. Protocol file `crates/net/protocol-changes/bot-spawn-team.md`.
- `sync_bot_minigames` applies it through `minigames.assign_team` (then
  ForceRespawn, as a rules bot put on a team). This happens when the bot
  joins its builder's game, after each new life (the brain is fresh after a
  reset or respawn), after load, and whenever the choice or the game changes.
  In between, SetTeam may move the bot. A slot the game lacks is retried. No
  choice leaves the team to the game.
- Brick bots are named `<kind> (<brick name>)`, else `<kind> (<team>)`,
  shortened to the 23-character name limit. Renames are quiet, with no
  "is now known as" chat.
- Wrench: a Team row (No team plus the builder's game teams; an unknown
  slot shows as "Team N" and is kept) is added above the vehicle spawn
  dialog's footer, styled like its Vehicles menu. The client sends
  `vehicle_team` and opens with the brick's team and builder. The content
  layout test's expected window height becomes `295 + TEAM_ROW`. That test
  needs generated content and was not run here.

## Tests (synthetic content; before = origin/main or the fix disabled)

All runs used a per-worktree `--config .cargo-wt.toml`, which sets one
unique `codegen-units` for every bri-* crate: 41, then 53, then 67 once
another worktree's build-script and include outputs leaked in under 53.
This keeps a shared target dir from mixing in another worktree's
workspace crates.

| Test | Before | After |
|---|---|---|
| `bot_soccer_teams::recipe_goals_bots_contest_one_ball_score_and_play_on_after_the_reset` (both sides, swapped) | fails: no grounded objective plan, targeted [0, 0] | passes: both target their own goal, contest more than 120 ticks, one scores, the scorer takes up the reset's replacement ball |
| `bot_interactions::a_mounted_driver_keeps_closing_on_a_retreating_target_beyond_its_walking_leash` (diagonal, renamed flank) | fails at tick 111, behaviour return | passes: gap 19.80 to 5.96 and 22.25 to 3.65 m, 0 reversing ticks, human retreat 55 m |
| `bot_soccer_teams::a_spawn_bricks_team_choice_puts_its_bot_on_that_team_across_save_and_load` | the field does not exist; with team application disabled it fails with (None, None) against (Some(1), Some(2)) | passes |
| `bri-ui` `vehicle_spawn_team_menu_offers_the_builders_teams_and_sends_the_slot`, `field_flow` synthetic | new; `field_flow` failed on the unaccounted Team menu until listed | pass |

Regression runs after the change are listed in the final section.

## 2v2 soccer

### Field and match

`crates/chaos/tests/bot_soccer_match.rs` builds a field on the Slate only
from stock v20 bricks:

- walls of 4x cubes, two high, with corner fillers;
- a goal pocket behind each mouth, 8 wide and 4 deep;
- in each pocket a non-colliding 4x4 plate whose detection region runs the
  Ball goals recipe's plain wrench events: `addTeamScore`, `winRound` at 5
  and a delayed `resetObject`;
- a kickoff Vehicle Spawn with the Steel Ball;
- a Vehicle Spawn per bot whose Team picks Blue or Red.

The host creates the game with teams Blue (slot 1) and Red (slot 2) and
loads the field. Matches run headless at 120 ticks/s for minutes, on fixed
seeds that move each pad by up to 2 units. The host resets the game three
seconds after a round is won, as the Mini-Game window's Reset does.

### Behaviour changes

All of them are in the contest piece and its tunables (`bots.json`), and
none checks a name:

- **Cover.** Before, a teammate's claim made the second bot stand down
  into Wander, the main idle source. Now it covers behind the ball and to
  the side, and steps off the path of a ball driven back at it.
- **Clear.** A ball an opponent drives straight back at a bot is knocked
  aside by `clear_degrees`.
- **Walls.** A push whose rear is blocked by world geometry turns off the
  wall.

### Metrics

Each match measures:

- goals per team;
- own goals: a pocket entry with no goal within 1.5 s either side;
- unattended ball (no bot within 6);
- ball on a wall (slow within 1.6 of a wall);
- ball lost;
- stuck (a goal more than 1.5 away, under 0.5 headway in 2 s);
- circling (a full turn in 2 s within 2 units);
- jitter (sharp turn reversals per bot-minute);
- ignoring the ball (Wander/Return with a ball in play);
- idle (still, more than 8 from the ball);
- clumping (two teammates within 2 of each other at the ball);
- slowest kickoff pick-up;
- facing away while in contact;
- wrong way (behind the ball as it moves toward its own goal).

Thresholds are in `assert_clean`.

### Before and after

The table covers 2v2 over 8 seeds of 180 s each, per kit. "Before" is
`cc6a330` behaviour with this test. Values are a mean per match, or the
maximum where marked.

| Metric | Hands before | Hands after | Hammer before | Hammer after |
|---|---|---|---|---|
| Goals per match | 14.8 | 14.5 | 16.5 | 15.2 |
| Both teams scored | 8/8 | 8/8 | 8/8 | 8/8 |
| Own goals (all seeds) | 7/118 | 10/116 | 7/132 | 6/122 |
| Longest unattended (max s) | 4.9 | 2.8 | 3.0 | 3.1 |
| Longest ball on a wall (max s) | 0.5 | 0.0 | 0.0 | 0.0 |
| Ball lost | 0 | 0 | 0 | 0 |
| Stuck (bot-s) | 17.1 | 7.1 | 17.1 | 10.7 |
| Circling (bot-s) | 0.26 | 0.02 | 0.02 | 0.18 |
| Jitter (max per bot-min) | 3.2 | 0.9 | 2.3 | 2.0 |
| Ignoring the ball (bot-s) | 281.8 | 25.0 | 264.7 | 22.6 |
| Idle (bot-s) | 160.3 | 1.6 | 159.5 | 3.7 |
| Clumped (s) | 1.0 | 0.1 | 1.9 | 1.1 |
| Slowest kickoff (max s) | 4.9 | 2.8 | 3.0 | 3.1 |
| Facing away (max share) | 0.07 | 0.01 | 0.07 | 0.00 |
| Wrong way (share) | 0.050 | 0.075 | 0.048 | 0.078 |

Over 720 bot-seconds a match, the before matches had each team's second
bot idle or wandering about a third of the time. Before also fails the
variations badly. In 3v3 the ball was unattended for 45 s and sat on a
wall for 67 s, with 715 bot-s ignoring it.

After, at 180 s and 2 seeds each, every variation passes:

| Variation | Goals | Own goals | Stuck (bot-s) | Ignoring (bot-s) | Idle (bot-s) | Clumped (s) |
|---|---|---|---|---|---|---|
| 1v1 | 13.0 | 4/26 | 5.4 | 11.4 | 0.2 | 0 |
| 3v3 | 13.0 | 2/26 | 18.7 | 28.3 | 8.7 | 3.5 |
| 1v2 | 14.0 | 2/28 | 5.7 | 17.6 | 0.5 | 0.1 |
| 2v3 hammer | 15.5 | 2/31 | 12.8 | 19.8 | 3.5 | 4.0 |
| 2v2, wall and ramp | 15.0 | 1/30 | 7.1 | 21.5 | 1.5 | 0.2 |

The short 60 s runs with a rocket launcher, a spear and parked jeeps all
keep the match going, with goals, no lost ball and no stuck brains.

### Tests

The `bot_soccer_match` tests, all of which fail on the before behaviour
(idle and ignoring):

- `two_against_two_play_a_clean_match_across_seeds`: hands and hammer,
  `BRI_SOCCER_SEEDS` (default 3) of `BRI_SOCCER_SECONDS` (default 150), and
  own goals at most a fifth of all goals;
- `other_line_ups_and_an_obstacle_field_play_on`;
- `rockets_spears_and_jeeps_keep_the_match_going`;
- `the_shipped_soccer_save_loads_a_ready_match`.

New unit tests in `bots/contest.rs` and `physical_objectives.rs` cover:

- the cover point and its slots;
- the cover leaving the worker's side;
- stepping aside from a ball driven back;
- the clearing turn;
- the wall turn.

### Loading the field (for Maxwell)

The save is `saves/Slate/Soccer 2v2.world.json` in the repository. It
holds only stock bricks (4x Cube, 4x4 flat plate, Vehicle Spawn), the game
and its teams, the goals' wrench events, the ball pad and four bot pads.
It ships the way the stock saves do: the `worlds` pack carries it
(`import_saves --bundled saves`), so a packaged build lists it in Load
Bricks under Slate as a "Bundled build". Nothing is copied by hand. To
load it:

1. Rerun `python tools/bootstrap.py` once after pulling: the worlds pack
   is rebuilt because its inputs changed.
2. In Add-Ons, turn on **Blockhead Bot** and **Steel Ball Kit**, and the
   Steel Ball if you want /clearballs.
3. Start a server on **Slate**. Do not be in a mini-game: the save brings
   its own.
4. Open **Load Bricks**, pick *Soccer 2v2* under Slate and load it. The
   Soccer mini-game is created with teams Blue and Red. The ball appears
   on the centre pad, and the four Blockhead Bots spawn on their pads and
   join their pad's team.
5. Join the game if you want to play: you are in it already as its
   creator. The bots kick off on their own. Five goals win the round;
   press **Reset** in the Mini-Game window for the next one.

To give the bots hammers, set slot 1 of the loadout to the Hammer in the
Mini-Game window.

### Limits

- **Load Bricks no longer drops a brick saved off its grid.** The field
  was authored against stand-in sizes of the stock bricks (the real Vehicle
  Spawn's size comes only with the generated content). A load placed only
  bricks whose saved position fits their definition's stud and plate grid
  (`fits_grid` in `publish_load_slice`), so a Vehicle Spawn of an odd stud
  count or an even plate count would have lost all five pads. v20 plants a
  saved brick wherever the save put it. Now a loaded brick off its grid moves
  to the nearest grid position, under half a cell (`Simulation::on_grid`,
  `Bounds::snapped`); one on it is placed as saved, as before.
  `the_shipped_field_survives_load_bricks_whatever_the_pads_size` decodes the
  shipped file as `Store::read` does and loads it with the host's Load Bricks
  command under five Vehicle Spawn sizes (8x8x1, 8x8x2, 7x7x3, 6x6x1, 5x3x2).
  Every brick survives within half a cell of its saved position: each pad
  with its bot or ball and team, both goals with their events and regions,
  and the game with its teams. With the old filter it fails.
  The gate's `bundled_soccer_field` test (real content, `--include-ignored`)
  lists the field under Slate from the worlds pack through `Store::new`, reads
  it as Load Bricks does, loads it on the real bricks and checks the same.
  It could not run here (no generated content).
- Own goals are credited by the engine to the last body that moved the
  ball. A defender blocking on its line is credited with a ball driven in
  off it, so a lone 1v1 defender concedes "own goals" (4 of 26). In 2v2 the
  rate is 7% (16 of 238), against 6% before.
- The wrong-way share rose from about 0.05 to 0.077. This is mostly the
  clearing turn's 60 degree knock, which can move the ball briefly back
  past the line. It is still under the 0.10 threshold.
- The rocket and spear runs show the match keeps going, but the bots do not
  shoot the ball. The physical provider's methods are push, hammer, hold
  and drive, with no "impulse at range" method. Adding one is a new
  behaviour subsystem, left for the next release. With weapon damage off
  they do not fight either. The jeeps stay parked: nothing makes driving
  one useful to the objective.
- The gravity-gun run needs the generated content (its tool is the stock
  Printer by reference) and was not run here.
- No offscreen frames were rendered. The render harnesses need the
  generated avatar and brick content, which is absent from this machine.
- In the obstacle variation, a ramp whose back face is exactly the walk
  step height (1.0) left bots pressed against it. The nav calls a rise up
  to step + 0.05 a walk, while the motor did not climb it. The variation
  uses a 0.6 ramp. The nav/motor step boundary is next work for the
  shared nav.

## Commands

```sh
CARGO_TARGET_DIR=/home/claude/bri-target CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 \
  cargo test --config .cargo-wt.toml -p bri-chaos --test bot_soccer_teams
cargo test --config .cargo-wt.toml -p bri-chaos --test bot_interactions
cargo test --config .cargo-wt.toml -p bri-sim --lib
cargo test --config .cargo-wt.toml -p bri-ui --lib wrench
cargo test --config .cargo-wt.toml -p bri-ui --test field_flow
BRI_SOCCER_SEEDS=8 BRI_SOCCER_SECONDS=180 cargo test --config .cargo-wt.toml \
  -p bri-chaos --test bot_soccer_match -- --nocapture --test-threads 2
BRI_BLESS=1 cargo test --config .cargo-wt.toml -p bri-chaos --test bot_soccer_match shipped
CLIPPY_CONF_DIR=<empty dir> cargo clippy --config .cargo-wt.toml --no-deps \
  -p bri-world -p bri-sim -p bri-chaos -p bri-ui -p bri-bls -p bri-client -p bri-addon-import -p bri-net \
  --all-targets -- -D warnings
```

`CLIPPY_CONF_DIR` was needed because a worktree nested under the main
checkout otherwise picks up an uncommitted `clippy.toml` from the main checkout.

## Remaining limits and next work

- `bots/interactions.rs` still has its own `DISCOVER` 24 for environmental
  opportunities (seats and loose hazards). It could read `objective_radius`
  or a separate kind field.
- Armed bots on an objective still fight a visible enemy first (Fight 0.8
  over Objective 0.65). Lowering `fight` in a kind changes that.
- The Team row's placement in the real v20 layout has only been checked by
  the content test's expectations, not run here; Maxwell's playtest should
  look at the vehicle spawn wrench.
