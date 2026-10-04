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

All runs used a per-worktree `--config .cargo-wt.toml` (codegen-units = 41
for every bri-* crate), so the shared target dir could not mix in another
worktree's workspace crates.

| Test | Before | After |
|---|---|---|
| `bot_soccer_teams::recipe_goals_bots_contest_one_ball_score_and_play_on_after_the_reset` (both sides, swapped) | fails: no grounded objective plan, targeted [0, 0] | passes: both target their own goal, contest more than 120 ticks, one scores, the scorer takes up the reset's replacement ball |
| `bot_interactions::a_mounted_driver_keeps_closing_on_a_retreating_target_beyond_its_walking_leash` (diagonal, renamed flank) | fails at tick 111, behaviour return | passes: gap 19.80 to 5.96 and 22.25 to 3.65 m, 0 reversing ticks, human retreat 55 m |
| `bot_soccer_teams::a_spawn_bricks_team_choice_puts_its_bot_on_that_team_across_save_and_load` | the field does not exist; with team application disabled it fails with (None, None) against (Some(1), Some(2)) | passes |
| `bri-ui` `vehicle_spawn_team_menu_offers_the_builders_teams_and_sends_the_slot`, `field_flow` synthetic | new; `field_flow` failed on the unaccounted Team menu until listed | pass |

Regression runs after the change are listed in the final section.

## Commands

```sh
CARGO_TARGET_DIR=/home/claude/bri-target CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 \
  cargo test --config .cargo-wt.toml -p bri-chaos --test bot_soccer_teams
cargo test --config .cargo-wt.toml -p bri-chaos --test bot_interactions
cargo test --config .cargo-wt.toml -p bri-sim --lib
cargo test --config .cargo-wt.toml -p bri-ui --lib wrench
cargo test --config .cargo-wt.toml -p bri-ui --test field_flow
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
