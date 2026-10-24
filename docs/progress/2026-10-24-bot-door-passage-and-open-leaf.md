# 2026-10-24 Bot passage recovery and physical open leaves

Maxwell requested actual battle-flow evidence with a dozen or two dozen zombies,
including doors, corners, stairs, crowds and visible-enemy engagement. The private
full-map diagnostic retains native brains, finite source defenders, ordinary
weapons/abilities and four source approach spawns. It starts twelve zombies and
permits growth to 24, within a shared owner-configurable living-NPC cap. Passive
human position beacons do not fire; accepted periodic healing can still fail to
save them. This is headless diagnostic evidence, not spectator acceptance.

The first 180-second trace exposes repeated noncombat movement against stopped
doors: Gd29 remains at Mover45 for 56-second windows, Gd15 at Mover110 for
41-second windows. Both native dispatch and private AA passage policy required
resting key zero, excluding a door obstructed halfway. A physical closing-door
regression fails with zero requests, then passes with one recovery request and
no crowd toggle. Native dispatch now permits stationary interrupted poses;
private passage policy handles these without changing human Use semantics.

The next full-map trace confirms the stopped doors can recover but exposes the
second contradiction: routes still cross open leaves. Gd18 presses against an
open Mover90 for 39.5 seconds without a visible enemy. All bots_open colliders
were ignored by navigation even at an open pose. The independent route check
first caches samples with a closed door, swings it open, then checks the proposed
route against its actual physical slab. It fails before with overlap, and passes
after the correction. An open leaf now participates in navigation; pose and
passability changes invalidate only affected areas through existing chunk
collision-change tracking. No new planner, unstick teleport or suicide is added.

External server-lag-report retains battle-dozen-before-16, battle-dozen-door-after-17,
blocked-door-before/after, open-leaf-route-before and door-route-after logs. The
two physical/route regressions pass after correction. Complete battle rerun,
required tiers/integration and human playtest acceptance remain pending. Neither
an increasing kill count nor the earlier diagnostic success closes battle flow.

The assembled 300-second trace (18) removes the earlier long movement stalls
for Gd29 and Gd18, but still records Gd15 at interrupted Mover110 for 49 seconds.
It does not establish complete door/crowd recovery or deeper facility routing.
After private NPC reload correction, capture 25 records sustained defender fire
beyond a magazine (several over 100 traced shots), while Gd15 still stalls up to
46.5 seconds. A human fixture beacon contributes to the separate Mover3 queue.
Keep these retained failures as the next bounded consumer work; no map-specific
offsets or damage/teleport recovery were added to force a battle victory.

Required focused validation passes in bot-spawn-door-focused-after.log: changed
crate clippy, tests of five crates and client unit tests, 935 seconds including
compilation. The first focused attempt caught a nonminimal boolean, corrected
without changing behavior. Full gate/main integration are the next required
step; this entry does not claim they have completed.
