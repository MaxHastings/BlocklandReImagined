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

An isolated two-player motor experiment placed one player on another and
compared the upper jump with the lower still or jumping, at five button delays
and both step orders. In this fixture the upper apex was 6.3463 in every case;
the jumping lower body only rose to 0.5795 before being obstructed. Thus simply
scheduling two jump presses does not currently produce the boost Maxwell asked
for. This does not prove all moving-support conditions fail; the shared motor
and its authoritative/predicted support-motion semantics need investigation
before implementing a general capability. The experimental code and output are
kept privately as `stack-synchronized-jump-experiment.rs` and
`stack-synchronized-jump-motor.txt`; they are not a passing boost test.

Checks: `cargo test --locked --profile playtest -p bri-sim
teamwork_is_default_and_a_kind_can_disable_it --lib`; `cargo test --locked
--profile playtest -p bri-addon-import hole_bots::tests --lib` (three cases);
and `cargo test --locked --profile playtest -p bri-chaos --test bot_brain
a_bot_that_sees_an_enemy_warns_its_side` passed. The alert integration test
compares enabled and disabled behavior. The cube trace is
`v043-cube-support-contact.json` in private diagnostics.
`cargo clippy --locked --profile playtest -p bri-sim -p bri-addon-import
-p bri-net --lib --bin bot_scenario_probe -- -D warnings` passed.
