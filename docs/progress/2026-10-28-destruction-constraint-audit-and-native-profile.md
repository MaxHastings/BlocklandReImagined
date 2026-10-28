# 2026-10-28 Destruction constraint audit and native profile

Active goal expanded by Maxwell: audit all destruction constraints and
optimizations, remove poorly justified visual/gameplay compromises, and profile
a seeded native Afghan Zombie CTF variant with rockets and more HE grenades,
without spears/swords. Keep original saves/installations unchanged and deliver
an updated playtest/PR/merge only after evidence supports the changes.

Read-through ledger: ../audits/destruction-constraints-2026-10-28.md. It records
each relevant bound, behavior at saturation, justification, correction or risk.

Implemented (not yet committed/merged): local contact-based sleep wake-up in
shared client Surroundings; removal of duplicate global/nearby caller wakes;
shared 1024-group metadata limit instead of the independent 64-reservation hold;
transient piece/cue/mutation quota deferral that retains an untouched hit/proof.
Added slow-frame awake, collision-waiting, staged-section and static counts.
Retained the accepted +6 dB source recordings and regeneration gain policy.

Checks so far: 19 destruction unit tests pass, including the 65th small collapse;
the temporary split retry passes for piece, byte and full cue quotas. Fifty
client debris/section/foley checks, 24 Add-On physics checks and 10 fade checks
pass (stress/endurance/soak excluded). Changed client/sim all-target clippy
passed before the subsequently added profiling scene; rerun for final sources.
The new overlap-insertion regression originally used an off-grid dynamic rest
pose as a planted brick, then exposed that static insertion needed explicit
local sleeper wake-up. Corrected authored-grid fixture and local broad-phase
insertion now pass support loss, unrelated sleeping, repaint and overlap cases.

The controlled debug pile comparison did NOT establish a useful performance
gain from wake locality alone: legacy/new simulation means ~8.210/8.164 ms at
60 Hz and 16.241/16.162 ms at 30 Hz; late awake ~2048/~1957. This is diagnostic
debug data, not shipping FPS. The fixture's finite floor and large throws also
need care when interpreting apparent non-sleeping. Do not tune sleep/solver or
claim a lag fix from those numbers. Report saved under ignored artifacts/
zombie-team-jets/pile-wake-paired-debug.log. The old optimized test binary is
preserved there as pile-baseline-tests.exe; initial optimized test compilation
was expensive. Normal warm debug checks then completed quickly.

Following Maxwell's explicit request for real-match profiling, extended the
existing entity_probe (no new crate) with an opt-in saved-match scene. It uses
dedicated::load_packages, native saved rules and normal bots, then the existing
real loopback server + offscreen App path and per-thread Windows stack sampler.
The two armed saved teams get rocket + four HE slots; spear/sword map pickups
become HE. Zombie team/kinds keep their unarmed policy. Native per-bot owner-ID
seeds stay deterministic; live network frame timing is NOT a bit-exact replay.
Input SHA/rule changes are recorded privately. Record actual observed projectile
types before assuming that the AI throws grenades with its new inventory.

The probe records worst client frames with named spans, native ticks, entities
and diagnostic notes; host/client observed projectiles; server damage counters.
It uses the shipping mimalloc allocator beneath its existing allocation counter.
Saved-match client defaults to 2048 bodies (override BRI_PROBE_MAX_BRICKS), not
fresh High/512, so the workload can match Maxwell's Best-quality logs. Default
probe scenes remain unchanged and do not require a private save.

Three 60-second native runs now completed. The usable native-DX12 baseline is
artifacts/zombie-team-jets/afghan-explosives-before-native-dxc/report.json:
114 rockets, 21 HE grenades, 11 tank shells; server mean 3.980 ms, max 15.732 ms,
120.002 ticks/sec. Worst client frames include 21-30 ms debris physics with four
120-Hz catch-up steps, zero collision-waiting bodies and zero/four staged groups.
The original save SHA256 stayed
e61edef5cb2b368e571fb06ebccfe4887c417fc82b9e27bb98ced37119532aa4.
The first Vulkan profile also localized work to contacts/solver, but is not the
shipping backend. The first DX12 attempt lacked the shipping compiler DLLs;
its 70-second join moved the match phase and is not a fair baseline. Corrected
native compiler DLLs/features reduced join to 6.953 seconds. The probe waits for
GPU completion each frame: its FPS is not interchangeable with interactive FPS.

Next controlled experiment: native Rapier parallel execution, retaining enhanced
determinism, timestep, iterations, bodies and geometry. Initial 512-body contact
and support-removal test passes worker-count state parity. A private opt-in
MessagePack capture/replay of the real debris PhysicsWorld avoids repeating an
entire GPU match for every comparison. Replay restores the same captured state,
warms four steps and measures four; checks identical final pose/velocity bits
across serial, one-, two- and four-worker paths. Eight steps fit the client's
two-frame collision look-ahead. Captured native content is ignored private
artifact data, not a game save/wire schema or repository asset. Performance is
NOT established yet; keep only a measured improvement. SIMD8 was rejected:
Rapier explicitly disallows it with enhanced determinism.

Current optimized probe build (parallel/serde features plus capture/replay):
artifacts/zombie-team-jets/physics-replay-build.log. Cargo check of this probe
passes. Feature changes rebuild dependencies despite the otherwise warm target.
Private runner run-afghan-explosives-profile.ps1 checks for an open game, can
capture the first actual >=15 ms physics frame, and verifies the save hash.

Final scope decision from Maxwell: stop the performance experiment and merge
the proven gameplay corrections and accepted audio only. First real-state
replay (1336 bodies, 3629 colliders, 2725 active contacts) gave serial/four-worker
four-step means 5.190/4.818 ms; one/two workers were slower, 11.843/7.342 ms.
All final state hashes matched. That modest isolated gain does not establish
an integrated FPS improvement. The replay omits the live frame's changing
inputs and Foley collector, so it is not a replacement for native frame costs.

Removed the parallel pool, dependency/feature changes, capture/replay APIs and
probe edits from the merge candidate. Investigation sources/reports remain
private ignored artifacts for traceability. Shipping physics rates, iterations,
body budgets and visible geometry stay unchanged. Wake locality is a verified
correctness cleanup, not a measured lag cure. The remaining recurring dense
debris cost is explicitly unresolved; no more profiling campaign in this task.

Maxwell then requested coordination with "Harden PR 387 Voice Chat", both lanes
on main and one combined private candidate for possible v0.4.0 approval. That
chat is handling stale generated Add-On bundle freshness after its VoIP merge;
root here coordinates one final package after the two lanes integrate. A public
release/tag requires Maxwell's later approval. Final scoped checks, remote PR/
CI/merge and native packaged verification are pending until recorded below.
