# 2026-10-28 Cosmetic debris solver cost

Follow-up to [the burst diagnostics](2026-10-28-destruction-burst-frame-cost.md).
Maxwell repeated the actual CTF/TDM match in preview.7 and reports it felt
mostly smoother, with a remaining dip during very large destruction. He asks
for one clear practical improvement and authorizes merge after required checks.
Preview.7 only added timings; its smoother run is not evidence of a physics fix.

## Actual capture

`E:/Downloads/Destruction-preview-2026-10-28-r7-diagnostics/BlocklandReImagined-v0.3.7-destruction-preview.7-windows/logs/session-20261028-151547.log`
identifies source `7f22a962a`. Recording is `match-101.brimatch` in the normal
application recording folder; all evidence/preferences remain read-only.

One post-load54.9ms frame has1,654 loose bodies,35 chunk bodies and103 section
members. Its four physics steps cost14.2ms of15.5ms debris update. Draw costs
31.9ms, with most of that span unattributed; it cannot all be assigned to physics.
GPU intervals have6.3/6.9ms medians and9.9/10ms maxima. Two focused intervals
average131/105fps, with6.1/9.3ms median frames. This capture did not reproduce the
previous25–45ms debris-update period. Separate577ms initial upload/calibration
and65ms chunk/avatar upload frames must not be conflated with steady destruction.

## Small correction

Only the `BrickDebris` cosmetic world uses two constraint-solver iterations
instead of Rapier's default four. Preserve120Hz fixed steps, CCD, all collision
geometry, impulses, material and Physics Quality body budgets. No adaptive
scheduler, moving-body replication or new setting. This trades precision in
visual rubble stacks for less solver work; authoritative physics keeps its
existing parameters. Add named draw spans for debris/fade uploads so a future
capture can account for that observed render-side gap without guessing.

The existing2,048-piece diagnostic now compares four/two iterations at both
60Hz and30Hz rendered updates, with nearby appearance edits every0.1s. Each
six-second run retains all2,048 bodies, requires finite poses and no floor
penetration for members over the floor, and reports simulation percentiles.
The first optimized A/B comparison measured mean simulation8.211→6.729ms at
60Hz (18% less) and19.148→14.576ms at30Hz/four-step catch-up (24% less).
Single-frame maxima were noisy, including a worse individual maximum in the
second case; do not promise an equivalent improvement in whole-game FPS or
claim every hitch solved. The final comparison adds percentile measurements.

Focused checks passed in83s: format, warnings-as-errors clippy and732 client
unit tests (78 ignored). The retained motion/collision/section checks pass with
the new default. All50 optimized debris tests also passed (11.98s test time),
including sustained motion, landing, collision, material and hit reactions.
The final four-step comparison measured mean16.776→12.982ms (23% less),
p9524.237→16.034ms (34% less), p9941.459→31.442ms. Its normal-frame mean was
essentially flat8.394→8.464ms; do not claim a uniform normal-frame/FPS gain.
These are same-seed2,048-piece fixture timings, not a replay of the full match.
Final-head full gate/CI, packaging and merge are next.
Human judgment of the remaining visual precision tradeoff
belongs to Maxwell; no interactive playtest was driven.
