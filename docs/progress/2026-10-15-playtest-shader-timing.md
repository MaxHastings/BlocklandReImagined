# 2026-10-15 Existing playtest shader timing evidence

Read existing session logs from the isolated playtest executable's target/release/
logs; no visible game or input was started for this inspection. The installed
executable predates current source, so these are historical observations, not a
benchmark of the final candidate.

- session-20261015-045245: FXC, scene pipelines 97,175 ms, content 3,920 ms.
- session-20261015-050408: FXC, scene pipelines 98,056 ms, content 6,065 ms.
- session-20261015-051636: FXC, scene pipelines 112,993 ms, content 15,052 ms.
- session-20261015-140424: DXC, scene pipelines 17,595 ms, content 9,944 ms.

The logs support a substantial compiler-related difference, but these were not
controlled same-state runs; do not claim a precise speedup ratio. No evidence
here attributes all time to Blood Gulch-specific shaders. SceneRenderer creates
shared pipeline variants, and gpu_build already logs their compilation separately
from content loading. The release packager pins/hashes dxcompiler.dll, and this
playtest's release directory now has that DLL. platform::open_device probes a tiny
pipeline and falls back to FXC when the runtime refuses DXC. Preserve that check;
do not force an unsupported compiler to hide the delay.

Exact final package startup/content load/shader logs remain required. The current
UV/material changes may alter compile time; old numbers cannot certify them.
Persistent pipeline caching or a renderer redesign has not been implemented.
