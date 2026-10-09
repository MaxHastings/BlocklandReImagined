# 2026-10-09 Sky and shading private preview

Branch: `codex/sky-shading-preview`, reusing the existing `preview` worktree.
Main is untouched. The Gravity Gun model merge c4d7a2e is reverted; the diff
against origin/main has no Gravity Gun package or client items.rs changes.
No main landing is authorized. Max must approve each feature PR by name;
any later landing uses `python tools/gate.py --push` on this PC.

Recovered and pushed the unfinished lighting_probe extension: independent
Classic/Unified/Dynamic loading, sky/shading variants, sun height, MSAA and
per-stretch GPU timing. Added configurable dimensions for 1440p measurements.
The first local build is still running; no verification claim yet.

Corrections in progress: generated sky scales by the map's authored daylight
reference (or explicit host light), softer low-sun saturation, larger sun disc,
no upward RGB channel change from Soft Shading, equal-depth emissive mask so
Glow and authored-unlit surfaces do not take AO. The mask follows geometry's
cut-out/clip/far-fade and per-sample coverage; blended surfaces remain after AO.
New checks cover glow with MSAA on/off, complete fog and floor brightness.

Next: compile/tests, paired baseline renders on 3c8ed1f, real-map montages,
GPU timings, mirrors timing, full local gate without push, private workflow
artifact with publish=false and the player checklist. All interactive
acceptance remains Max's; the full alpha contract is open.
