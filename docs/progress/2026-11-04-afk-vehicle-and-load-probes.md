# AFK vehicle and loading verification

Maxwell authorized an unattended validation goal while AFK on November 4.
Interactive playtests remain his; these probes never create a window or send
OS input. Original installations, user saves and other worktrees are untouched.
The full alpha contract is not declared complete.

The additional seat integration target exposed a stale JSON fixture missing
the required driver-braking field introduced by #433. The fixture exercised
seat geometry, not deserialization. It now constructs a typed VehiclePose so
the compiler checks future required fields. All original scale, turn, anchor,
invalid-transform and node-composition assertions remain unchanged. Four
seat-fallback, seven pose-composition and three synthetic first-person vehicle
cases pass; three explicit generated-content variants stay ignored here.

The explicit external-asset Speedkart render now exercises ten consecutive
states from front and rear: steering endpoints, neutral, switched lamps,
braking, reversing, JumpHeld, empty braking/reversing and destroyed lighting.
Actual authored lamp nodes/offsets, completed color ramps, finite cone records,
GPU light upload and declared grip nodes are checked. Rear pixels distinguish
braking from neutral, match the authored brake/reverse condition and retain
neutral visuals for JumpHeld and empty driver-only conditions. Twenty captures
pass; representative lit-front, braking-rear and destroyed-rear frames were
visually inspected. Two separate point/dynamic-cone GPU regressions pass.
This is not interactive driving acceptance or a full source-game fidelity claim.

These explicit private-asset/save probes are now classified with the existing
manual-only gate entries. Ordinary content-free fixtures still run, and no
assertion or failure was hidden. The private Speedkart and three required
zombie-save cases were actually run with their inputs; external files remain
outside Git. Missing proprietary inputs must not accidentally break an
unrelated extended gate. The 55 gate-tool tests pass.

Added `map_load_probe`, an explicit CLI with `test=false`, avoiding an empty
libtest harness. Set BRI_LOAD_CONTENT, BRI_LOAD_MAP and BRI_LOAD_OUT, then run
`cargo run --locked --profile playtest -p bri-client --bin map_load_probe`.
It uses fresh private state, the normal client's allocator and initial 1280x720
framebuffer, a loopback host and actual offscreen rendering. Each process
measures the first map selection plus two disconnect/rehost selections. The
clock ends only after InGame, an authoritative player pose and a completed
world frame. App startup and GPU initialization are separate; no artificial
shader warmup is inserted before selection. Per-phase loading logs and GPU
identity go in the report. Optimized loading measurements remain pending.

The destruction stress findings and new candidate stay in the separate
[open destruction PR](https://github.com/MaxHastings/BlocklandReImagined/pull/437).
Its motion still needs Maxwell's judgment. An A* tie-order experiment reduced
balcony pauses but failed the retained all-three tower assertion and made
tower arrival slower, so it was reverted. No bot behavior change is shipped
from that experiment. The baseline three saved pursuit checks and 22 ordinary
bot-brain integrations pass; some expensive balcony detour searches still
pause for several simulated seconds. That is retained planning latency, not
evidence of completed optimal traversal.

Private evidence is under
`E:\Downloads\BlocklandReImagined-Release-Evidence\afk-validation-2026-11-04`.
The updated give-back Speedkart archive remains unchanged and all assets,
scripts, credits and provenance are preserved.
