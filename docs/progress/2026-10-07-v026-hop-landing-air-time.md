# 2026-10-07 Hops judge their landing over a real jump's air time

The merge gate failed ledge pushes merged onto main (gate log
`6f2f91827e95.log`): `rooftop_brawl_without_rails` reported `fell=1`
("no bot strafed off the deck"), the same on retry.

## Cause

A trace of the falling bot's last 400 ticks showed it fighting about 3.4
units from the deck's edge and walking toward it at 4.2 u/s. It then hopped
(vy 12) and came down past the edge. No weapon pushed it: its last push was
300 ticks earlier. `hop_lands` judged where a hop lands after a fixed
`HOP_FLIGHT` of 0.8 s. A jump from level ground goes up at `jump_speed` 12
and comes back down under `gravity` 20, so it is in the air 1.2 s and every
hop carried half again as far as judged. Main's change that stops corpses
from blocking players altered the fight's paths enough to reach that hop.

## Fix

`hop_lands` now takes the body's tuning and judges the drift over
`2 * jump_speed / gravity`. `HOP_FLIGHT` is deleted.
`hop_tests::a_hop_is_judged_over_the_time_a_real_jump_is_in_the_air` jumps a
real player and checks that air time against the motor (within 0.1 s). It
also checks that a hop toward an edge is refused where the old constant
would have passed it.

## Review follow-up: jetted hops

The dodge hop holds its jets for `HOP_TICKS` when it has them, and the jet
dodge always does after its crouch. Jet thrust beats gravity, so such a hop
is in the air longer than a plain jump. `hop_flight` now takes the seconds
of jets fired: up at `jump_speed`, climbing at `jet_acceleration` less
gravity while the jets fire, then back down under gravity. The dodge judges
its "up" ways over `HOP_TICKS` of jets when it has jets. A second test,
`a_hop_that_fires_its_jets_is_judged_over_its_jetted_flight`, checks the
model against a real jetted hop within 0.1 s. That tolerance is three of the
motor's 32 ms ticks, the most landing can be seen late.

## Evidence

The branch is ledge pushes (ec7db637) merged with origin/main d068f06e,
whose tree is identical to the gate's 6f2f9182, plus the fix.

- `cargo test -p bri-chaos --test bot_gauntlet`: 18 passed. Rooftop:
  fell=0, knocked_off=0, kills=52, team_kills=0.
- `bot_gauntlet all_dials_on -- --ignored`: passed.
- `bot_tactics`: 16 passed. `bot_perception`: 5 passed.
- `bri-sim` bot tests: 193 passed.
- rustfmt is clean, and `cargo clippy --workspace --tests -- -D warnings`
  is clean.
