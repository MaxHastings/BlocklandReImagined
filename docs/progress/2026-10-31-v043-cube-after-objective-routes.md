# Cube baseline after objective and floor-route changes

After `a8335539b`, replayed the supplied `ZombieCan'tGetMeOnTopOfCube` save
through `bot_scenario_probe`, using the user's coordinates and ordinary saved
minigame/bot behavior. Five bricks loaded, zero were set aside. In 120
simulated seconds the target remained alive at 10 health; no stacked kill or
reliable cooperative climbing is established. The one-second snapshots contain
39 samples where a bot's motor reported a living actor as its support. One
upper zombie spent consecutive sampled seconds on the same ally while pursuing
or searching, usually with no waypoint while static navigation searched; a
subsequent waypoint below its feet then made it leave. Actual damage occurred,
so neither “bots never stack” nor “the cube is solved” describes the result.

Command: `target/playtest/bot_scenario_probe.exe content <private cube save>
90.3241 294.6100 98.5718 14400 <private report>`. The complete report is
`v043-cube-current-120s.json` in the pinned release-evidence diagnostics.
An initial 7200-tick run ended with 70 health and multiple observed support
contacts. These are headless samples, not an interactive test.

The previous physical paired-jump experiment remains a limiting result: with
the upper actor already on the lower one, the lower actor's jump was physically
blocked by the body above and the upper actor gained no extra height in the
tested timings. Correct cooperation needs both a dynamic-support action plan and
an authoritative, prediction-consistent treatment of moving supports; merely
keeping a bot on a head or asking both to jump does not establish the requested
capability. Existing body collisions, ceiling safety and normal routes must
remain intact. Continue under the design and acceptance matrix in
`2026-10-31-v043-npc-design-review.md`.
