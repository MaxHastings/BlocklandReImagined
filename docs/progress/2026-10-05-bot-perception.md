# 2026-10-05 Bots notice things: glances, reactions, fair aim, human turns, one sight budget

Lane `fix/bot-perception`, approved by Max: a small, general "what a bot
notices" layer. Base `claude/project-thread-pt64ji`, merged up to
7b34b7b9 (teamwork and tuning tools).

## What changed

- `crates/sim/src/session/bots/perception.rs` (new):
  - **Glances.** An idle bot (Wander or Return; no enemy in sight, no
    objective at hand, nothing held, not seated or driving) turns its
    ordinary aim for about `glance_seconds` at the most salient of: a
    projectile blast (10 units of reach per unit of radius), a weapon sound
    (12 units at full volume), a 1.5 s stare within 8 degrees, a body
    faster than twice the bot's running speed (both within 0.3 of the
    kind's `sight`), and, at a low chance, anyone of any team close by in
    plain view. `salience` scales every reach; salience is the chance of a
    glance; `cooldown_seconds` follows. The look-round runs on the bot's
    own `cadence` beat.
  - **Reaction.** A newly seen target, or an attacker it was not fighting,
    gets `reaction_seconds` (fighting or hunting), times `relaxed_scale`
    (strolling, interacting), times `away_scale` outside its `view_degrees`
    cone, +-30%, seeded. The same scale widens the starting aim error;
    outside the cone it also turns `away_scale` times slower until it has
    reacted. A spawn-protected target is watched, not reacted to. Damage
    still interrupts at once; only the return fire waits.
  - **Fair aim.** However long it tracks, the aim trails a target moving
    across its line of sight (relative to the bot) by up to 0.5 s of that
    motion times `strength` (`steady_error`), at most 0.3 rad either way:
    a strafing target is missed by the same distance at any range, a still
    one is hit as before. (Uncapped to 0.6 rad, a close strafer drew 18 of
    331 CTF shots more than 25 degrees off it, over the gauntlet's 1 in
    20.) The
    native fire gate (`bot_hand_fire_gate`) now judges a shot by where the
    bot believes it aims (`perception::believed`: its look without its
    error), so the error is a real miss instead of a withheld shot; before,
    `validate_fire` only admitted shots that would hit, which is why the
    fair metric read 100% for the gun whatever the error.
  - **Hurt from out of sight.** `guess`: the incoming direction to within
    25 degrees, the distance to within 40%, never nearer than a unit to
    the truth; the look holds until the reaction; allies get the guess;
    sight gives the exact spot.
  - **Warnings.** Each ally acts on a warning a seeded 0.25-1 s later (per
    ally and warning via `cadence::spread`), at a spot up to 1.5 units off.
    A warner repeating itself does not put the delay off (it first did:
    `bot_brain::a_bot_that_sees_an_enemy_warns_its_side` caught it).
  - **Turning.** `State::turn`: speeds into a big turn (top 1.3 x
    `turn_degrees`, acceleration 1.8 x rate squared, so a half turn takes
    about the plain time), eases out, overshoots a fast flick a few degrees
    and settles within a degree. Handling things (Carry, Objective,
    Interact) keeps the plain turn and is not held by a startle. A
    strolling look drifts up to 7 degrees.
  - `delay_ticks` / `Brain::switch_delay` for a chooser's tell.
- `crates/sim/src/session/bots/sightlines.rs` (new): one sight-ray budget
  per tick (`bot_sees`, `bot_sees_player`): 128 rays for ordinary queries,
  4 per bot (sized for 32) for its target or attacker. Ordinary answers are
  cached per (viewer, subject) for 6 ticks while neither end moves half a
  unit. Eye, else chest. Vehicles with seats occlude, except the viewer's
  and subject's mounts and a body the viewer pushes; seatless bodies do
  not (a ball hid soccer opponents: the 2v2 hammer seed 3 wrong-way share
  went 0.082 -> 0.106 with it, over the 0.10 bar; base passes). Routed:
  combat sight, perception polls, arming item checks.
- `crates/sim/src/session/bots/cadence.rs`: copied from
  `fix/bots-ball-games-2` (37c290a7); perception's salts are 101-104.
- `BotKind::perception`: seven knobs, on by default: `salience`,
  `glance_seconds`, `cooldown_seconds`, `strength` (0-4, 1 shipped; the
  one dial, scaling reaction delay, starting error, view-cone delay and
  turn cap, warning delay, turn overshoot, drift and the steady aim error),
  `relaxed_scale`, `away_scale`, `view_degrees`. `strength` is the name the
  tuning tools find by themselves; `bot_tuning.json`'s fair dial is now
  `perception.strength` with sign -1 (more human, fewer hits).
- `fair_hit_rate` now enforces the band (all classes together, and the gun
  and the bow each); `fair_by_dial` asserts monotone.
- `bots.rs`, the teamwork lane's "stand out of a teammate's fire" step:
  taken only where there is floor at the spot (the strafe's own probe).
  On the rooftop deck its spot was off the edge; a bot walked off, wandered
  below for the rest of the round, and the six-unit deck read idle 9.8%
  (bar 1%). With the probe: idle 0.0%, no falls, 57 kills.
- Weapons runtime `Blast` event; weapon sounds and blasts feed
  `Bots::notice`. `BotThought::noticed`, also shown in `why()` while under
  way.

## Decisions

- No combat discount on reaction: a 0.6 combat scale doubled deathmatch
  behaviour switches.
- A flat steady error floor (3.25 x `aim_error_degrees`) reached the band
  too, but made a still target miss (the gravity gun failed to catch a
  standing builder from 18 units); the tracking lag only costs against
  motion.
- Hurt uncertainty does not depend on `strength`.
- Other lanes' sight checks are not edited (team.rs untouched); their
  one-line swaps are in the lane report.

## Evidence

Commands (shared target dir, a per-lane codegen config so no other
worktree's artifacts are reused): `cargo test -p bri-sim --lib bots`,
`cargo test -p bri-chaos --test <suite> -- --include-ignored` for every
bot suite, `cargo clippy -p bri-sim -p bri-weapons -p bri-chaos --tests
-- -D warnings` (clean), `cargo fmt --all`.

Fair metric at the shipped `strength` 1 (steady hit rate; band 15-60%,
enforced for all, gun and bow):

| weapon | 10-12 units | 12-25 units |
|---|---|---|
| gun | 57.4% | 38.3% |
| rocket | 35.3% | 15.0% |
| shotgun | 5.6% | 10.5% |
| bow | 44.4% | 51.6% |
| bouncer | 0.0% | 0.0% |
| all | 28.7% | |

`fair_by_dial`: `perception.strength` 0.5 / 1 / 2 gives 38.2% / 28.7% /
20.9%, falling as the dial rises. Before: gun 100% from the first second,
not monotone.

Passing: `bri-sim` bots unit tests (136), `bot_perception` (19),
`bot_brain`, `bot_physical_objectives`, `bot_tactics`, the other bot
suites, and the gauntlet except as below.

Expectations changed, with the reason in each test:
- `deathmatch_mixed_arsenal` kill bar 50 -> 40 (48 kills; 67 on the
  release, when nearly every shot landed).
- `water_between_the_sides` switch bar 20 -> 25 (21.6 a bot-minute with
  19-22 kills; the release has 10.8 with 12, the same per kill).
- `an_actual_attacker_can_interrupt_a_retained_delivery`: return fire must
  land; the bot need not out-duel a scripted attacker who never misses and
  shoots first.

Still failing, not loosened:
- `a_jeep_on_each_side` (one round): clumped 4.0%, stuck 9.5%. Every
  clumped sample is a bot on a jeep roof beside its ally seated in that
  jeep (the scorer counts seated neighbours); the stuck share is one bot
  wedged on a parked jeep for 800 ticks. Chaotic: at strength 0 it passes,
  with the cone open or a linear turn it fails; the old base failed it
  too, and the release fails it over four rounds (circling 4.0%).
- `bot_soccer_match` hands seed 2: wrong way 0.116 (bar 0.10; release
  0.094, strength 0 0.095, a linear turn 0.047, other variants 0.13-0.16).
  All the wrong-way pushes are in Objective, which already turns plainly.
- `all_dials_on` (in `gate-known-failures`), `bot_think_time_16` (the
  release also fails it on this loaded machine: 36979 us vs 42485 here).
- Content-dependent tests (no generated content here) fail at
  `crates/package/src/testing.rs:9` on both trees; `showcase
  a_bot_carries_its_catch_out` fails on the base as well.

## Next

- Team and surprise lanes: swap their rays to `bot_sees` /
  `bot_sees_player`; the surprise lane's goof option belongs in
  `Alertness::of`'s relaxed arm.
