# 2026-10-27 Complete local collapse assemblies

Continuing draft PR #379 after Maxwell's pirate-ship mast playtest. The
screenshot alone cannot identify the cause, but code inspection confirmed two
independent omissions: charging section members to the rigid-body limit, and
rejecting all members after initial landing. Maxwell requested a focused fix
and another private Windows playtest, preserving little bricks while making
physics chunks larger. No interactive play session was automated.

Presentation now separates original visible meshes, shared assembly transforms,
and independent physics bodies. Complete section totals, bounds and a lowest
footprint near the hit accompany existing removal cues; a new protocol-change
file requires matching server/client builds. Late members join the current
pose and breakup waits for complete delivery. Interrupted groups keep their
received art together through the normal fade. Tall shapes lean around a base
before falling free; broad floors drop/tilt and narrow spans rotate. Contact
uses eight cached original-member corners, with 256 shared candidate steps.

Connected region growth starts during the fall and resumes within 4096 cheap
candidate steps/frame. Landing groups keep original geometry and use one box
proxy each. Quality controls granularity; at most 64 assembly bodies share the
existing rigid-body budget with ordinary debris. Sleeping/budget-evicted bodies
keep their art at the last pose. Collision streams within swept oriented bounds
under 512 shared steps/frame (terrain tiles cost 16), pausing motion until ready.
Approximate assembly proxies ignore each other to avoid false collisions through
empty sail/beam bounding space. Existing local pushing, shots and blasts also
reach these bodies. No pose replication, crushing, or gameplay rubble.

The separate render reservation is 65536 members, independent of Physics
Quality. Overflow rejects new whole groups instead of cutting arbitrary members
out of admitted assemblies. The existing per-look mesh/texture instancing is
reused. This remains bounded cosmetic presentation, not unlimited skyscraper
physics. Full 13-second lifetime includes falling and normal final fade.

Up to eight proven server groups drain round-robin under the existing global
128-removal/8192-query tick budgets. Shape metadata accumulates during support
proof, avoiding another unbounded full-component geometry scan. Both authored
edits and direct damage invalidate affected queued proofs. Discovery remains
incremental; the change removes delivery serialization, not proof latency.

Targeted client regressions passed: all 1500 connected mast members retained
through falling/landing at an eight-body limit; late post-landing arrivals keep
the current pose; quality zero preserves admitted assembly art; expiry removes
bodies; mast leans before contact while a broad floor drops. A server regression
checks two 160-member groups both emit in the first 128-removal round with
consistent complete-shape metadata, and subsequent rounds remain capped.

## Delivered private preview

Engine commit `1fba39778f2fd26702295150dbf3764634006e98` is pushed to
`codex/plastic-destruction-playtest`, draft PR #379. Optimized release build
`v0.3.7-destruction-preview.2` completed in 17m01s. Client: 101499904 bytes,
SHA-256 `cd9ceff97a7032f063cf0fadb363f637b6224bd5af58574186ceee5a91132237`.

Launch `E:/Downloads/Destruction-preview-2026-10-27-r2/BlocklandReImagined-v0.3.7-destruction-preview.2-windows/Launch.cmd`.
The installed v0.3.7 folder and preview.1 were not overwritten. This preview
contains physics changes only, separate from the concurrent jet-audio chat.
The jet chat was informed of this exact destination here for coordination.

Actual awaited packaged startup exited 0: 21 maps, 1396 brick definitions,
35 save pictures and 14 modern light sets. No game window/audio device opened.
Existing optional Butterfly Knife Add-On warnings remain. All 13173 packaged
file hashes, 88 unique package IDs/directories and ZIP CRCs were verified after
startup canonicalized the package config. ZIP: 234334584 bytes. The folder
contains PLAYTEST-DESTRUCTION.md; its parent contains the source/hash receipt.
Maxwell can retry the mast now; gameplay and visual acceptance remain his.

Checks: all three focused mast/client regressions and nine server destruction
regressions passed. Focused all-target clippy and final changed-library clippy
passed. The full dependent integration suite (five crates) passed. Client unit
sweeps each had 706 passed, 78 ignored and one unrelated ragdoll failure:
soft-cone or dead-body settling; each passed its exact isolated retry. An earlier
client sweep passed all 707. These are flaky retries, not clean sweep passes.
The first server fixture used an invalid 1x1 grid center; corrected to quarter
stud coordinates and its final nine tests passed. Initial style lints were
corrected before the engine commit.

The integrated gate passed history and Python tool checks but was interrupted
while building at Maxwell's request to prioritize playing now. No gate compiler
processes remained after interruption. It has no full-pass record; rerun before
merge, alongside Windows CI and Maxwell's acceptance. No main merge occurred.
Logs are under ignored artifacts/collapse-assemblies/.

Handoff timing lesson: target/ and sccache were warm, but choosing release
still paid thin-LTO work. Use the repository's existing incremental playtest
profile for the next private iteration, and begin its build alongside scoped
checks. Avoid holding the playtest for unrelated broad checks. The release
profile was an unnecessarily slow choice for this private turnaround.
