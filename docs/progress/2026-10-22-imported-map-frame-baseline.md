# Imported-map submission baseline and deterministic scope fixture

Maxwell reports lag in the Interdiction preview. Its actual focused session
minute averages 31 FPS / 30.2 ms median, with CPU update 11.86 ms, recording
8.20 ms and submission 11.47 ms; GPU median 11.7 ms. These overlap. The preview
mistakenly shipped profile dev (client unoptimized; render/sim optimized at 2).
The existing optimized playtest client builds in 6m10s and passes the assembled
content check. It has not replaced the delivered preview; no measured FPS
improvement is claimed.

The offscreen capture harness now prints existing RenderStats with each saved
frame. An optimized underground wall view submits 1,958,370 triangles in 3,226
draws, 2,457 translucent draws and zero bounded-scene culls. Three Pipeline
interior views submit 602,694–623,520 triangles / 373–424 draws, also with zero
bounded-scene culls. This counter is not an assertion that avatar culling is
absent: avatars have a separate view check. Static maps remain merged/unbounded;
scatter has distance/fade selection without camera-frustum selection, and
blended placements can generate separate draws. Measure the first generic
submission correction against retained forest/interior frames, preserving
secondary shadow/reflection views. Captures/counts are not a quiet-machine FPS
benchmark, and source appearance remains separately open.

Two failed capture attempts are retained privately: a concurrent content check
locked the Windows executable during Cargo relinking, and the private capture
tool decoded UTF-8 stderr with cp1252, losing its diagnostic stream. The tool
now explicitly decodes UTF-8 and selected captures run sequentially. No gameplay
window or user input automation is used.

The full gate on headroom candidate ae2249a17 exposes an unrelated existing
scope fixture race: Reload expires after 64 ticks before the simulated click
arrives under contention, so the host legitimately fires in Ready (11 -> 10).
The isolated retry passes. The fixture now holds each barred state until its
explicit Ready command; initial equip, redraw and respawn explicitly advance
after observing Draw. This retains ordinary input, refused shots, held RMB,
focus/swap/death/respawn assertions without a half-second observation window.
The first correction omitted the later redraw/respawn advances and correctly
failed; the complete correction passes the targeted test in 4.63s. Broader
tiers and integration of these diagnostics/test-only changes remain pending.

The complete scope fixture and capture diagnostics pass quick in 63s and
focused in 104s (661 client unit checks, clippy and formatting). PR298's
headroom candidate passes the full gate in 2,193s, including 539 test binaries
and the 32-bot release budgets (4.01ms < 7.00ms; 3.50ms < 5.50ms). All Windows
checks pass; it is merged into main bddc1cc0a558642891a84b2649bd1dac670256b8.
Diagnostics/fixture changes remain a separate candidate for the next generic
map-submission correction. The repeated scope check has no new fixed-duration
observation window. No production culling behavior has changed yet.
