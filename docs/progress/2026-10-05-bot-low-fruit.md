# 2026-10-05 Bot extras, seeded looks and first names (fix/bot-low-fruit)

Max: "keep things as simple as possible then capture all low hanging fruit
to round out the experience". From the "cheap ways to widen options" list
in `bots/npc-edge-cases.md` and two on-camera tells from
`bots/robotic-tells.md`.

## What changed

- `crates/sim/src/session/bots/extras.rs` (new): five extra options, each
  through ordinary controls and weighed by one number in the kind's
  `extras` (`bot_kind::EXTRAS`, default 1, 0 off). See the "Extras" section
  of `docs/architecture/bots.md`.
  - `idle_play`: the Interact opportunity no longer needs an enemy. With
    none about, a push on a loose body toward the nearest player in sight
    (stopping short of them) and a passenger seat in a teammate-driven
    vehicle score 0.2 x weight, between Wander and Return. A rider keeps
    its seat while a teammate drives. Hooks: `interactions.rs`
    (`bot_interaction`, `bot_opportunity`, `act_bot_interaction`, the
    Wander dismount).
  - `crouch`: hurt from more than 5 units while fighting or holding its
    ground, crouch 1.5 s.
  - `dodge`: a harmful projectile (damage or splash damage), not its own or
    an ally's, whose predicted path (velocity, ballistic fall, splash
    radius) meets the body within 0.75 s: a jump, with jets if the body can.
  - `activate`: a swap brick or a brick with an enabled `onActivate` row in
    the way to its goal (2.5 units) is clicked with the empty hand; now and
    then at a natural pause one within 8 units in sight is visited and
    clicked for fun.
  - `hand_weapon`: with two or more attacks and an unarmed teammate in
    sight, walk up, face them, drop a spare their way (`Command::DropTool`).
- `crates/sim/src/session/bots/looks.rs` (new): every bot a seeded look
  (face and decal from the avatar pack, clothing colours from the server's
  opaque paint colours, limbs paired, skin kept, the kind's `look` on top);
  brick bots named by a free first name of their kind, kept while they live
  (the old "Kind (Label)" form remains the fallback).
- `bot_kind.rs`: `extras` map and `EXTRAS` list, validated 0 to 10.
- `bots.rs`: module lines, the `extras` brain field, one call after the
  surprise act, the aim/jump/crouch/jet merge; `embody_bot` applies the
  seeded look; `brick_bot_name` takes first names.
- Took only `bots/cadence.rs` from 37c290a7 (fix/bots-ball-games-2); the
  extras' periodic checks use `cadence::beat`. The rest of that commit (fire,
  alert, hop, dismount and respawn phases) is left to its own branch: on
  this base it alone fails `bot_soccer_match`
  `other_line_ups_and_an_obstacle_field_play_on` (2v2 obstacles, wrong way
  0.13 and 0.10), checked by applying the commit to an export of
  e1e2c30c. `mod cadence` carries `#[allow(dead_code)]` until those users
  land.
- A brick with its own name keeps "Kind (name)" (scenes such as
  `bot_soccer_teams` find their bots by it); only unnamed bricks take first
  names.
- Two fixes found by the existing suites:
  - `dodge` first hopped while running, which carried bots off the rooftop
    deck (gauntlet `rooftop_brawl_without_rails` idle 8.8%,
    `zombie_survival` switches 16.9). A hop is now straight up (`stand`) and
    holds off a weapon hand-off; the gauntlet is 13/13 again.
  - `idle_play` let 3v3 soccer bots push the ball toward a player between
    goals (seed 2: 3 own goals of 4, wrong way 0.10). Bisected per option
    with `BRI_SOCCER_ONLY=3v3`; only `idle_play` failed. A trace showed the
    harm came from the first second, before the planner gave anyone an
    objective. Idle play now waits until its game has had no objective in
    play for 10 s, and never takes a body another bot claims.

## Tunables added

`extras.idle_play`, `extras.crouch`, `extras.dodge`, `extras.activate`,
`extras.hand_weapon` in `packages/blockhead_bot/assets/bots.json`, all 1.

## Evidence

Tests in `crates/chaos/tests/bot_extras.rs`, each against the same scene
with the option off or its cause absent:

- `an_idle_bot_near_a_loose_ball_pushes_it`
- `a_bot_under_ranged_fire_crouches_more_than_one_not_under_fire`
- `a_predicted_hit_triggers_a_hop_more_often_than_a_shot_that_misses`
- `an_activatable_door_on_the_route_gets_activated`
- `a_spare_weapon_ends_up_with_an_unarmed_teammate`
- `bots_of_one_kind_get_varied_seeded_looks`
- `brick_bot_names_come_from_first_names_with_no_duplicates`

Unit tests: `extras::tests` (path prediction, splash and fall, chance),
`looks::tests` (varied, paired, deterministic looks).

All with every extra on (the shipped defaults), on Linux, debug build:

- `cargo test -p bri-sim --lib`: 232 passed, 2 ignored.
- `bot_extras` 7/7; `bot_soccer_match` 5/5; `bot_soccer_teams` 5/5;
  `bot_gauntlet` 13/13; `bot_brain` 18, `bot_interactions` 18,
  `bot_physics_interactions` 11, `bot_physical_objectives` 14,
  `bot_objectives` 11, `bot_tactics` 13, `bot_knowledge` 8,
  `bot_navigation_spike` 8, `bot_creator_acceptance` 8,
  `bot_creator_adversarial` 5, `bot_carryable_objectives` 7,
  `bot_search_objectives` 4, `bot_objective_rest` 2,
  `bot_creator_heldout` 1, `shark_policy` 16, `turret_seats` 3,
  `gun_seat_fire` 1, `spawn_blocked` 1: all pass. The perf and firefight
  tests are `#[ignore]` and were only compiled.
- `cargo clippy -p bri-sim -p bri-chaos --all-targets -- -D warnings`:
  clean. `cargo fmt --all`: clean.

## Left out

- Head aim, k-best routes and the other list items: not in this lane.
- Idle play needs a player in sight to play toward; a lone bot does not
  kick a ball about by itself.
- Activation for objectives stays with the objective planner; this lane
  only adds the route and flavour clicks.
