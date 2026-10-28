# 2026-10-28 Destruction burst frame cost

Maxwell reports preview.6 is much better, but the entire game sometimes slows
during a large CTF/TDM firefight with simultaneous destruction. This is a new
frame-cost report, distinct from the preparation stalls corrected in PR381.
Use the existing clean, merged playtest worktree and its warm target directory.
Branch: `codex/destruction-burst-performance`, based on main `92579c315`.

## Evidence

Read-only session evidence:
`E:/Downloads/Destruction-preview-2026-10-28-r6/BlocklandReImagined-v0.3.7-destruction-preview.6-windows/logs/session-20261028-143212.log`.
The session identifies preview.6 source `ccdcd0eb8`, RTX4070 SUPER/DX12,
and long gameplay frames with brick-debris update costs25.0/29.0/30.7/33.3/
39.4/45.1ms. Several usual-frame averages are40–58ms. Separate250–586ms
map/pipeline/upload spikes must not be attributed to the debris simulation.
Saved settings select Best,2048 bodies; settings and recordings remain unchanged.
The top-level debris span includes cues, local queries, preparation and physics,
so the old log cannot identify which inner stage dominates those exact frames.

Added a few nested frame-trace spans for surroundings, assemblies, collision
preparation and simulation, using the existing always-on tracer. No new telemetry
system. A repeatable2048-body diagnostic retains every member, runs360 frames,
and compares a quiet build with one nearby appearance edit every six frames.
First debug run: physics13.66/14.19ms per frame; preparation2.16/2.20ms;
assembly/surroundings work negligible. Around1952 bodies remain awake late in
both cases. This fixture has a dense pile and does not reproduce the entire
recorded firefight or establish shipped-build timings.

Code review found no-op mutable accesses for every ordinary body on every fixed
step (`set_enabled(ready)`), plus the equivalent assembly path. Rapier tracks
mutable accesses as modified bodies; each calls `rigid_body_updated`, bumping
the active-set epoch used by solver caches even when enabled state is unchanged.
This is a supported optimization candidate, not yet a measured cause of the
reported45ms frame. Local collision-history scans and global wakeups during
world edits are further candidates; do not weaken collision or motion to guess
at the fix.

## Checks / next work

`cargo test -p bri-client --lib stress_continuous_world_edits_with_best_quality_rubble --locked -- --nocapture`
passed (11.84s test time), with stage output in ignored
`artifacts/destruction-burst/profile-before.log`.
Optimized comparison (same playtest profile as the shipped preview) completed:
baseline simulation7.93/7.71ms per frame; the guarded-write candidate8.09/9.08ms.
The awake counts were identical. This does not support that candidate as the
main cause; removed its ordinary/assembly changes. These are controlled-fixture
measurements, not timings of the recorded CTF session.

Maxwell explicitly asked whether this was guessing or log-informed and offered
a real-world capture. Explained the distinction: the45ms debris span is real;
the exact inner cause is not yet proved. Prepare a diagnostic preview with the
same gameplay rather than ship an unproved performance adjustment. Its existing
frame trace now separates incoming changes, projectile/pusher queries, model
readiness, collision preparation, assembly work and simulation. The bounded
formatter shows a third nesting level and fixed-step counts; without that change
the new inner spans would not appear beneath update/world presentation/debris.
Slow advances also note loose/chunk body counts, section members and fixed steps.
No extra toggle, new logging system, collision reduction or motion change.

Quick passed731 client units (78 ignored). Final focused checks passed in88s:
format, warnings-as-errors clippy,732 client units (78 ignored), including the
new deep-log formatting check. No gameplay correction is claimed or merged;
the full integration gate remains for the eventual measured correction.
Next: package the instrumented copy, have Maxwell
repeat the actual CTF workload, then use those inner timings to choose a fix.
A new video is not needed for CPU attribution. No interactive session was driven.
