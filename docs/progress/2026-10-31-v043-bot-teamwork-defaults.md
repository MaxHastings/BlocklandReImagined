# NPC teamwork defaults and stacking evidence

Maxwell clarified that general NPCs should cooperate by default, consistent
with the existing bot-kind behavior controls. `alerts_allies` now defaults on:
it shares an observation with eligible nearby allies, preserving the observed
position and expiry rather than revealing unseen live movement. An explicit
source `hAlertOtherBots = 0` imports as false; an unspecified setting uses the
native default. `cooperative_stack` likewise now defaults on, and explicit
numeric `hSuperStacker` values are preserved. This replaces the earlier
default-off assumption in the stacking handoff. The stacking movement itself
is still not implemented.

The actual pinned Bot_Zombie archive imported with `cooperative_stack: true`;
its generated `bots.json` differs from the existing private package only by
that setting. The imported package and report are under the private planning
evidence folder `imported-zombie-stack-20261031a`. It was not installed or
committed. The cube probe now records motor contact actor IDs; a 3600-tick
trace recorded bot 4 standing on bot 2 for 384 consecutive ticks, but no
second level. The target had 85 health at the end of this shorter trace.
The contact field is diagnostic and a future controller must revalidate live
support, team and collision eligibility.

An initial isolated two-player motor experiment placed one player on another and
compared the upper jump with the lower still or jumping, at five button delays
and both step orders. In this fixture the upper apex was 6.3463 in every case;
the jumping lower body only rose to 0.5795 before being obstructed. **This initial
experiment did not step the shared physics world and is not valid evidence of
normal gameplay.** A corrected experiment that steps the world after both motors
measured an upper apex of 6.3088 in each tested case, with the lower body's feet
reaching 0.0151. Its checked delays and step orders do not produce the requested
boost. This does not prove all moving-support conditions fail; the shared motor
and its authoritative/predicted support-motion semantics need investigation
before implementing a general capability. The experimental code and output are
kept privately as `stack-synchronized-jump-physics-ticked.rs` and
`stack-synchronized-jump-physics-ticked.txt`; they are not a passing boost test.

For the separate climb onto an ally, the existing `Body::leaps.seconds`,
`Ground::sweep_arc`, and `route::LeapLeg` already measure reach, check fixed
geometry, and drive ordinary jump and air-steering controls. Reuse those for a
live, eligible actor support rather than inventing a separate movement motor.
They do not validate the support's lifetime, team, movement, dynamic obstacles,
or coordinated support behavior; those checks and a bounded state machine are
still required. In particular the no-world-floor recovery currently sends a
perched actor back down and must be overridden only during a valid cooperation.

Checks: `cargo test --locked --profile playtest -p bri-sim
teamwork_is_default_and_a_kind_can_disable_it --lib`; `cargo test --locked
--profile playtest -p bri-addon-import hole_bots::tests --lib` (three cases);
and `cargo test --locked --profile playtest -p bri-chaos --test bot_brain
a_bot_that_sees_an_enemy_warns_its_side` passed. The alert integration test
compares enabled and disabled behavior. The cube trace is
`v043-cube-support-contact.json` in private diagnostics.
`cargo clippy --locked --profile playtest -p bri-sim -p bri-addon-import
-p bri-net --lib --bin bot_scenario_probe -- -D warnings` passed.
