# 2026-10-03 v0.2.3 sustained battle causal baseline

This lane preserves the strict activity acceptance and both failed receipts.
It uses headless authoritative controls only; Maxwell owns interactive playtests.
No original installation, generated content or production code was changed.
Root serialized the optimized build/replay against other Cargo/GPU work on the
16 GiB Apple M1 Pro Mac. `CARGO_BUILD_JOBS=2`, `CARGO_TARGET_DIR` unset.

## Inputs and exact replay

Source baseline: `0eebedb81ad3d3990b0277dfe4b2518ce1a63e2b`.
The retained `/tmp/bri-v022-final-content` snapshot exactly matches the prior
receipt: 9,471 files, 392,843,334 bytes; aggregate SHA-256
`161735207b50e5abe574a5b86fca698c9cd969210f74271888a7cd6e98a16a92`.
The aggregate hashes sorted relative paths, a NUL separator and each file's
binary SHA-256. Published Blockhead objective policy and the installed actual
`gamemode_slayer_ctf-rules` query provider remain loaded. No flag sources are
authored, so this measures empty-offer discovery, not CarryReturn grounding.

```sh
env -u CARGO_TARGET_DIR CARGO_BUILD_JOBS=2 \
  BRI_CONTENT=/tmp/bri-v022-final-content \
  BRI_BATTLE_WORLD='Beta City 16' BRI_BATTLE_BOTS=12 \
  BRI_BATTLE_RANGED_SIDES=1 BRI_BATTLE_CTF_PROVIDER=1 \
  BRI_BATTLE_TICKS=36000 BRI_BATTLE_RELOADS=2 \
  BRI_BATTLE_REQUIRE_ACTIVE=1 BRI_BATTLE_STOP_AFTER='rocket battle' \
  cargo test -p bri-chaos --release --test bot_battle_perf \
  profile_sixteen_bots_in_the_real_bedroom -- \
  --ignored --exact --nocapture --test-threads=1 \
  > /tmp/bri-v023-beta12-causal-replay.log 2>&1
```

Optimized build: 3m39s; test execution: 81.43s; exit **101**. The failure
prevents the second actual city construction/run. The selected-out Bedroom
construction counters are not two successful city reloads.
Executable `target/release/deps/bot_battle_perf-fbdd013351ea4bb0` is
24,391,040 bytes, SHA-256
`2cab977ea0c5f3896ddedb78fba9307ade904d669687c7fd052884cfd9994bbf`.
Full provenance is retained at `/tmp/bri-v023-battle-provenance.json`.

Current source includes later NPC/native-control changes after the old
`20fa89e3` receipt. Therefore this is a current-source baseline, not an exact
old-source reproduction or a before/after timing comparison. The original
`/tmp/bri-v022-sol-final-beta12.log` remains an unresolved failed receipt.

## Evidence and causal classification

| Phase, 36,000 steps | Gun | Rocket |
|---|---:|---:|
| Step p50 / p95 / p99 / max, ms | 1.029 / 1.482 / 1.678 / 7.107 | 0.762 / 1.254 / 1.524 / 2.718 |
| Steps >5 ms / >50 ms | 1 / 0 | 0 / 0 |
| Active steps | 35,568 | 35,998 |
| Fight controller ticks | 76,600 | 215,565 |
| Observed health loss / deaths | 40,820 / 404 | 25,948.6 / 263 |
| Distinct observed live fired-projectile IDs | 1,942 | 315 |
| Strict failed windows | none | 21, 22 |
| Gameplay evidence fingerprint | `f0e9876c15034647` | `ef17e582c6d9f088` |

Every Gun window passes. Every current-source Rocket window records actual
health loss; the old zero-damage window does not recur in this different
source baseline. That does not establish its old-source cause or a fix.

Both failing current-source Rocket windows omit exactly bot **8**. Each has
1,200 living ticks, zero dead ticks, zero respawns, zero visible-enemy ticks,
zero participating ticks and 1,200 Ready Rocket-image ticks. It spends
1,199/1,200 ticks in Search (the other tick is Wander in window 21).
The two end poses have identical feet `[100.34621, 287.60745, 98.19085]` and
zero velocity, grounded, with unchanged spawn tick 60,692 and full health.
Both report `bounded search exhausted`, no goal/waypoint and no active route.
All 12 spawner bricks survive. Bot 8 participates again in window 23 and
fires an observed live projectile in window 24, then ends the phase fighting
elsewhere. No permanent silent brain, respawn wait or exhausted ammo is proven.

Read-only source analysis identifies the control chain: `Situation.remembers`
is true for any retained memory; Search scores 0.4 above Return 0.3 and Wander
0.1. `Brain::pursue` returns no destination after bounded evidence probes are
exhausted, leaving hold-and-look control. Allied sightings keep this memory
fresh: bot 8's endpoint evidence advances from subject 12 at tick 63,029 to
subject 13 at tick 64,200 while its feet remain unchanged. This supports an
occluded search/recovery investigation. It does not establish that the actual
body has a feasible navigation start or escape; that must precede a production
correction. No policy weight, search/physics budget or threshold was lowered.

## Timing and resource boundaries

Gun's worst step is phase tick 0, the first step after MiniGame creation;
Rocket's worst is phase tick 16,195. Neither is a measured stall above 50 ms.
Native AI/movement/physics/weapons/event costs are still combined in the full
`Session::step`; this run cannot assign their individual causal shares.

Existing package VM telemetry is included in step timing. Gun totals are
Slayer 318.912 ms, CTF 51.044 ms and Gravity Gun 682.764 ms across 36,000
steps; Rocket totals are 248.726 / 54.354 / 689.929 ms. These counters exclude
native state/snapshot cloning and query/model preparation and are not query
counts. They do not support a measured production bottleneck correction.

Full snapshot/MessagePack diagnostics are outside step timing. Gun snapshot
p99/max is 0.039/0.058 ms; MessagePack p99/max 3.077/3.213 ms. Rocket snapshot
is 0.044/0.054 ms; MessagePack 3.192/3.231 ms. This is diagnostic full-world
encoding, not the host's incremental network workload.

Sampled RSS: Gun start 253,728 KiB, maximum 255,552 KiB, end 180,704 KiB;
Rocket start 180,704 KiB, maximum/end 180,864 KiB. Sparse samples are not a
true peak or a leak test. No client/GPU memory, Windows frametime/render stall,
Windows speedup, NaN-crash fix or comparison to v0.2.1 is claimed.

## Next bounded diagnostic

A harness-only proposal is retained at `/tmp/bri-v023-battle-trace.patch`.
It is not applied or compiled yet. `git apply --check` and standalone rustfmt
parsing pass. Opt-in trace selects one existing controller at 1 Hz across at
most six ten-second windows, reports its real pose/vitals/thought, nine
independent local navigation start samples using the real archetype tuning
and scale, and at most 16 nearby live brick bounds. It uses a separate nav
cache after the step timer and changes no authoritative control/world state.

The next same-input chronology should add `BRI_BATTLE_TRACE_BOT=8` and
`BRI_BATTLE_TRACE_WINDOWS=20:24`, preserve `REQUIRE_ACTIVE=1`, and retain any
failed receipt. Root owns compute ordering and production integration. The
lease for the completed baseline was explicitly released; no further Cargo,
GPU or sustained run is active in this lane. Gravity/Spear/mixed phases,
actual second city reconstruction, additional worlds/controller counts,
populated-provider combined performance and old-source zero-damage causality
remain open. The full alpha contract remains incomplete.

## Later playtest: delayed brick return hitch (static audit only)

Maxwell reports initial brick destruction/debris physics is smooth, then a huge
hiccup around 10–20 seconds later that recovers; he is approximately 90% sure
it coincides with actual brick return. The actual game/settings and quantity
are still needed. This is new client performance evidence, independent of the
stationary bot's Search windows. No reproduction or improvement is claimed.

The authoritative deadline path is `Session::step_events` → `respawn_bricks`
→ `Simulation::mutate_many`. A rocket blast's destruction and its due return
both mutate a batch and refresh contacts once; compound chunks deduplicate
changed membership before one flush. Restoration is outside the existing
event watchdog's timed scope, so a quiet slow-event log cannot exclude it.
Restore must additionally retire each hidden brick's own sensor collider:
`sync_flags` currently calls `Parking::remove` one handle at a time. That is
an audit candidate, not a measured bottleneck. The client prediction mirror
already retires changed collider handles as a batch.

Client `Building::sync_world_changes` updates incremental query indexes and
increments its query generation once per applied change set. Cosmetic
`Surroundings::sync` reacts by clearing all cached nearby brick/terrain
colliders and waking every debris body; any remaining bodies then repopulate
nearby colliders. This can happen at brick restoration even when initial
physics was smooth. Separately, all same-age debris expires at 13 seconds
(10 seconds solid plus 3 seconds fade) and bodies/colliders are removed in
one `BrickDebris::age` pass. Ordinary minigame brick return defaults to 30
seconds; event `fakeKillBrick` has its own authored delay. A user-defined
10–20 second delay could put either or both epochs inside the reported range.

CPU world meshing runs in a background job and dirty chunks are deduplicated,
including cover-neighbor chunks. When the job lands, the render thread
reserves and uploads **all** pending chunks in one frame. Geometry uploads
validate/copy/split each scene and write pooled buffers; an aggregate reserve
requires one contiguous range and retains any newly allocated pool blocks.
Restoration can therefore cause a concentrated CPU/upload cost or temporary
pool growth despite batched host collision. Neither a scheduler cap nor a
pool change is justified before stage measurements.

Existing `debris_probe` permanently removes bricks and times cosmetic expiry;
it does not restore them or exercise ordinary server respawn. Existing
`entity_probe blast` follows actual rockets and returns but summarizes all
frames together. A focused replay needs canonical destruction/restore
commands, receipts containing observed restoration ticks and counts, and
separate host-step, client collision mirror, Building query, debris advance,
background chunk build, pooled GPU upload/complete timings. Record worst
frames with before/after body/collider/chunk/pool counts and distinguish the
13-second cleanup epoch from the observed return epoch. It must preserve
controls, deadline, all returned bricks and collision/query correctness;
no gameplay/frame-rate conclusion follows from Mac headless timings alone.

## Independent authored-unlit review

Root's Dynamic authored-unlit loader change preserves texture/tint, opaque
texture-alpha override and alpha/mask dispatch. Lit housing remains VertexLit;
LegacyCompat still assigns Surface. Water/sky loader paths are unchanged.
A downstream map-only shadow filter excluded Unlit, so formerly opaque
Surface props would cease blocking Dynamic sun-map/lamp-map/light-cube rays.
Root accepted that review finding. The proposed correction and bounded
coverage regression are retained at `/tmp/bri-v023-unlit-shadow.patch`,
with production source untouched by this lane. The whitelist addition is
inside Dynamic's `light_cubes` branch; blend/background exclusion and mask
coverage remain authoritative. The offscreen test compares identical
Surface/Unlit blocker geometry under sun and point light, and checks zero-alpha
opaque, solid/empty masks, blend, sky and water. It is not compiled/run by this
lane; root owns integration and the compute queue.

Root approved a focused diagnostic-only new auto-discovered client binary,
retained as `/tmp/bri-v023-respawn-probe.patch` (not applied by this lane;
no manifest changes). Standalone rustfmt parsing and `git apply --check` pass.
It installs actual content-catalog `fakeKillBrick` rows through trusted admin
authoring for the nearest requested visible solid non-water bricks, fires
`onActivate`, and advances canonical moves/steps through the native delayed
return. The first proposed serialized run is:

```sh
env -u CARGO_TARGET_DIR CARGO_BUILD_JOBS=2 \
  BRI_RESPAWN_BRICKS=128 BRI_RESPAWN_SECONDS=15 \
  cargo run -p bri-client --release --bin respawn_probe -- \
  /tmp/bri-v022-final-content /tmp/bri-v023-respawn128-cpu.json \
  'Beta City 16' > /tmp/bri-v023-respawn128-cpu.log 2>&1
```

The same inputs with `BRI_RESPAWN_GPU=1` add pooled geometry/debris upload
and bounded 256×144 offscreen completion. All observed selected bricks must
actually die and return with visible/colliding/raycast flags, observed
restoration must match the authored deadline within two ticks of the 60 Hz
client sampling, and both mirrors must retain restored geometry. Per-stage
p99/max, all frame receipts, worst twenty frames, body/collider/chunk/pool
counts and exact return/expiry epochs are retained. Background meshing is
costed synchronously here and explicitly is not added up as App frametime.
No map/avatar/UI/shadow or Windows interactive frame-rate claim is made.

Initial individual event outputs refresh each brick separately, so those
identified setup-kill frames do **not** reproduce rocket batch destruction
performance. Automatic deadline restoration reaches the same batched
`respawn_bricks` path as rockets. This is a bounded causal return/cleanup
probe, not a substitute for actual rocket performance or the user's eventual
interactive playtest. Root owns applying, compilation, runs and provenance.

Root subsequently measured the proposed shadow fixture against the unfixed
filter: formerly Surface-blocked receiver 0, Unlit receiver 212, expected
failure retained in `/tmp/bri-v023-unlit-shadow-before.log`. Root applied the
one-line Dynamic whitelist fix. The full lighting suite initially reported
18 passed, 1 failed, 1 ignored: the new negative Water control lacked required
native uniforms and was rejected by source validation (no panic). This lane
prepared `/tmp/bri-v023-unlit-water-fixture.patch` to derive valid uniforms
through `water_scene::append(Water::volume(...))`; root retains the complete
failed receipt `/tmp/bri-v023-lighting-suite.log`. No validation was relaxed.

Root's corrected full lighting suite now passes **19 tests**, with the one
real-map offscreen inspection ignored: receipt
`/tmp/bri-v023-lighting-suite-final.log`. Authored-unlit shading and opaque/
masked occlusion now both have positive/counterfactual evidence; sky, water
and blended map blockers remain excluded. No changed shadow preference or
LegacyCompat material assignment is inferred from the Dynamic-only fix.

Root explicitly released compute to this lane for the first 128-brick CPU
respawn probe. Before build, the retained snapshot was reverified against
its aggregate SHA-256 above; current HEAD is still `0eebedb81...` but source
has root/NPC integrated modifications. Current tracked-crate/manifests source
aggregate is `2aa84a3ae30451bc615d2c91f9e9d1fb077677639ef315cfd4cdf01897a1fca4`.
Initial probe SHA-256 is
`4c4aa88ec72181c579aa80279c369a2082d189d4e6d03ba4e0bda6ca72a1c06e`.
Provenance is `/tmp/bri-v023-respawn128-provenance.json`. The exact proposed
128-brick/15-second command is now running; final outcome remains pending.

## 128-brick canonical respawn CPU receipt

The exclusive run is complete and the compute lease was explicitly returned
to root. No further Cargo/GPU run was started. Root may run its correctness
queue while this lane reads receipts.

The first compile found a probe-only wgpu 30 descriptor API mismatch. A first
fix exposed that `InstanceDescriptor` no longer implements `Default`; the
probe now uses the same `new_without_display_handle_from_env()` constructor
as existing headless probes. Both failures remain at
`/tmp/bri-v023-respawn128-compile-failure.log` and
`/tmp/bri-v023-respawn128-compile-failure2.log`.

The first executing attempt returned exit 1: **50 of 128** bricks died.
This came from the canonical per-owner schedule quota, which rejected the
remaining simultaneous inputs; receipt
`/tmp/bri-v023-respawn128-quota-failure.log`. The fixture correction stages
32 `onActivate` inputs per 60 Hz client frame, drains their scheduled jobs,
then admits the next cohort. It retains the all-128/deadline/query guards
and changes no engine limit. Thus initial kills and eventual returns each
span four cohorts across **50 ms**, not one simultaneous 128-brick batch.
That distinction matters when comparing to an actual rocket blast.

The corrected command above passed, exit 0; final retry release build took
2m10s. Receipt `/tmp/bri-v023-respawn128-cpu.json`, log
`/tmp/bri-v023-respawn128-cpu.log`. All 128 selected bricks actually died,
returned visible/colliding/raycast on the expected authored deadline within
sampling tolerance, and both client query/collision mirrors retained the
restored geometry. No production code or acceptance threshold was changed.

| Epoch | Observed frames / simulated seconds | Measured relevant CPU costs |
|---|---|---|
| Canonical event setup kills | 10–13 / 0.1667–0.2167 | 32 bricks/frame; 32 collision refreshes/frame (individual outputs, not rocket-batch fidelity) |
| Cosmetic expiry | 790–793 / 13.1667–13.2167 | Debris advance 0.172 / 0.312 / 0.229 / 0.133 ms; 32 bodies expire per frame |
| Scheduled return | 910–913 / 15.1667–15.2167 | One host refresh/frame; host two ticks 0.418–0.470 ms; mirror 0.113–0.163 ms; Building 0.039–0.053 ms |
| Isolated chunk meshing at return | same four return frames | 3.780 / 3.488 / 3.851 / 5.186 ms; 2 / 2 / 2 / 3 chunks rebuilt |

Return rebuilt 32,604 / 34,212 / 36,000 / 47,580 vertices. Its largest
cost is background mesh work, costed synchronously only for isolation here.
This does not establish the client's actual frame cost. GPU upload, encoding
and completion are **unmeasured**: the disabled probe's JSON stage arrays
contain zero placeholders and adapter is null, not measured zero GPU costs.

Debris peaks at 128 bodies, 128 awake, 279 cached nearby statics and 217
contacts. Cleanup leaves no bodies before return; cached statics clear on
frame 794. Therefore this receipt does **not** test restoration's collider
invalidation while live debris remains. Prediction colliders transiently
reach 713 and finish at 583 after restoration, with retained geometry checks
passing. This single cycle is not a general leak/long-run growth test.

Global maxima across 1,080 frames: host two ticks 1.114 ms (initial setup,
not return); collision mirror 0.169 ms; Building 0.063 ms; debris advance
0.856 ms; isolated mesh 5.186 ms. The delayed huge hitch is not reproduced
for this 128-brick CPU fixture. No measured evidence supports scheduler
budget changes or a production host optimization.

Runtime tracked-source aggregate (605 files) remained unchanged during the
corrected build/run:
`1ee8799970374abe867ec60f32763f570ee73620c37f086d6ab996cdf4787a15`.
This hash includes runtime `src` and manifests, separating parallel
uncompiled chaos-test/documentation edits. Final probe SHA-256 is
`79f8ca6a9b5c43027cf866ca9575cb18bf21886ed2cd1d416f685f31b52a814e`.
Executable `target/release/respawn_probe`: 26,956,912 bytes, SHA-256
`226cff6441951e54d1868e07bb292b656a6e719f9e84edc978f0fe9e61d6616f`.
Provenance remains `/tmp/bri-v023-respawn128-provenance.json`.

After root's remaining correctness queue, the next relevant measurement is
same-input offscreen GPU upload/completion, then a justified larger/batched
rocket restoration if needed. A 12-second control can test return with live
debris, without changing cleanup policy. The sustained bot geometry trace is
still pending. Windows interactive frametime, the user's exact explosion
quantity/settings and actual simultaneous rocket-batch cost remain open.

### Same-executable 128-brick offscreen GPU receipt

Root granted an exclusive direct-executable lease; no Cargo or source edits
occurred during this run. The lease was explicitly closed on completion.

```sh
env BRI_RESPAWN_BRICKS=128 BRI_RESPAWN_SECONDS=15 BRI_RESPAWN_GPU=1 \
  target/release/respawn_probe /tmp/bri-v022-final-content \
  /tmp/bri-v023-respawn128-gpu.json 'Beta City 16' \
  > /tmp/bri-v023-respawn128-gpu.log 2>&1
```

Exit 0, all 128 canonical knockout/restoration/deadline/query guards pass;
Apple M1 Pro Metal. Provenance is
`/tmp/bri-v023-respawn128-gpu-provenance.json`. Verified retained executable
SHA-256 is `226cff6441951e54d1868e07bb292b656a6e719f9e84edc978f0fe9e61d6616f`.
It and its exact probe source are also preserved at
`/tmp/bri-v023-respawn128-retained-executable` and
`/tmp/bri-v023-respawn128-retained-source.rs` before another probe build.

The same four 32-brick return cohorts on frames 910–913 cost isolated mesh
3.679 / 3.770 / 3.922 / 4.979 ms; pooled chunk upload
1.461 / 1.613 / 1.724 / 2.116 ms; encode/submit
0.396 / 0.356 / 0.322 / 0.320 ms; offscreen GPU completion
1.238 / 0.930 / 0.929 / 1.076 ms. Host two ticks at return were
0.463–0.519 ms, collision mirror 0.108–0.187 ms and Building 0.045–0.052 ms.
No delayed huge hitch was reproduced in this bounded offscreen subset.

At cosmetic expiry frames 790–793, debris advance was
0.205 / 0.404 / 0.267 / 0.179 ms and GPU completion
0.779 / 0.815 / 0.758 / 0.770 ms. All debris was gone before restoration.
Global GPU completion maximum was 2.537 ms at initial kill frame 10
(p99 1.630 ms), rather than cleanup or return. Pool usage grew from two
blocks / 21,031,040 bytes on frame 0 to four / 63,093,120 bytes on initial
kill frame 10, then remained constant through cleanup, return and end.
That is measured initial replacement allocation, not measured delayed growth.

The GPU replica had 274 peak cosmetic cached statics versus CPU 279, with
small awake/contact differences. Ordinary adaptive cosmetic budgeting and
physics can depend on wall time; source/control/deadline equality does not
assert an identical cosmetic gameplay state. This 256x144 subset excludes
map rendering, avatars, UI and shadows; synchronous mesh costing remains
separate from App's background meshing. No Windows frametime claim follows.

### Prepared single native explosion burst (not yet run)

`/tmp/bri-v023-respawn-burst.patch` changes only the diagnostic binary.
It leaves the prior event-cohort mode available and adds
`BRI_RESPAWN_MODE=explosion`. Root applies it; this lane has not edited
shared probe or production source for this preparation. Rustfmt and
`git apply --check` pass; compilation and execution remain unverified.

The fixture loads a native build through ordinary `Command::LoadBuild`,
drains its actual published slices outside timed frames, creates a minigame
with explicit 15-second brick respawn, installs one ordinary wrench
`spawnExplosion` row and fires one `onActivate` input. It chooses native
small-plate geometry and a projectile by mechanical volume/radius coverage,
without editing the content pack, projectile force/radius, event quota or
cleanup policy. It verifies all candidate brick boxes and excludes unrelated
city bricks before emitting a geometric proof in the log.

For 512 1x1x1 plates, the 8x8x8 grid occupies 4x4x1.6 m and has per-brick
volume 1. The farthest brick box is about 2.572 m from the chosen centre
brick. Both unchanged radius-3, max-volume-30 projectiles in this snapshot
cover it; deterministic mechanical selection picks the lexically first,
`v20.projectile.gravityrocketprojectile`. The actual run must prove this
choice and all 512 native knockouts/returns, including one observed batch
for each and the unchanged 15-second deadline. Synthetic geometry sits
20 m above the highest saved brick to exclude unrelated save destruction;
it is a focused batch test, not an exact comparison to the prior city subset.

The relevant causal candidates are one authoritative restore/contact refresh,
client collision mirror, Building index changes, chunk rebuilding/upload,
and pool allocation at restoration. Cosmetic expiry is separately expected
13 seconds after the burst; with a 15-second deadline no live-debris
invalidation cost is covered. Production scheduler budgets remain unchanged
until a stage is measured as causal. Root owns the compute queue; no new
Cargo/GPU run was launched for this preparation.

### Independent map occupancy patch review while waiting for compute

Root requested read-only review of
`/tmp/bri-v023-map-occupancy-production.patch`; no shared engine/test source
was edited by this lane. The baseline closed-floor hanging-brick admission
is a real failure in `/tmp/bri-v023-map-occupancy-before.log`.

The proposed signed nearest-surface check has a sound basis for consistently
wound closed manifold solids, including closed concave or disconnected closed
components. Testing each convex collision part's centre avoids using an empty
compound's overall centre. It leaves collision sweeps two-sided.

However, `MAP_TAG` also marks static shape collision meshes, and the native
interior adapter accepts open/disconnected triangle surfaces. Pseudo-normal
availability from `FIX_INTERNAL_EDGES` does not establish a closed solid.
Parry 0.31.1 `shape/trimesh.rs::compute_pseudo_normals` explicitly requires
closed outward manifold geometry and warns about points closest to open
boundaries. A single broad AABB spanning two open panels can include free
space; a nearest upper panel's boundary normal can classify that free space
as behind the panel even though no filled volume exists.

`/tmp/bri-v023-map-open-control.patch` is a prepared ordinary planting
negative control (not run by this lane): two upward open panels at
x[-10,-8]/y0 and x[2,4]/y2 share one native-adapter trimesh; an owned supporting
brick permits a new brick at x[0,1], centre y1.1 in the free gap. The proposed
query sees a nearest upper edge with +Y normal and signed offset about -0.9;
its floor exception also fails because logical top1.2 is below surface2.
Expected result is ordinary placement remains legal. Root should establish
this control before applying unrestricted solid-side interpretation.

Maintainability/performance: `ConvexPolyhedron.mass_properties(1.)` calls
`to_trimesh()` and integrates its faces, allocating/integrating again per
candidate map collider and convex part. A cached occupied point, or a convex
piece's arithmetic vertex centroid, avoids requiring inertia computation to
answer placement occupancy. This is a source-level cost observation, not a
measured bottleneck or a proposed respawn optimization. No Cargo/GPU run was
launched for this review.

### Single native 512-brick burst CPU and GPU verified

Root granted a new exclusive compile/run lease, held production runtime
source static, then explicitly received the lease back after both runs.
Only the diagnostic bin patch and its compile/fixture corrections were
applied by this lane. Corrected complete patch against the retained 128-brick
source is `/tmp/bri-v023-respawn-burst-verified.patch`.

First compilation failed on double-reference map indexing in a `min_by`
closure; diagnostic-only dereference fix, retained log
`/tmp/bri-v023-respawn512-burst-compile-failure.log`. First execution failed
its all-512 native load guard with zero published bricks: the invented
1x1 plate centres were off the native stud/plate grid. The correction snaps
the origin to legal .25-stud/.1-plate centres and preflights every `Bounds`
before the unchanged ordinary LoadBuild command. Failure remains
`/tmp/bri-v023-respawn512-burst-grid-failure.log`. No engine admission,
projectile parameter or all-brick/deadline guard was weakened.

```sh
env -u CARGO_TARGET_DIR CARGO_BUILD_JOBS=2 \
  BRI_RESPAWN_MODE=explosion BRI_RESPAWN_BRICKS=512 BRI_RESPAWN_SECONDS=15 \
  cargo run -p bri-client --release --bin respawn_probe -- \
  /tmp/bri-v022-final-content /tmp/bri-v023-respawn512-burst-cpu.json \
  'Beta City 16' > /tmp/bri-v023-respawn512-burst-cpu.log 2>&1

env BRI_RESPAWN_MODE=explosion BRI_RESPAWN_BRICKS=512 \
  BRI_RESPAWN_SECONDS=15 BRI_RESPAWN_GPU=1 \
  target/release/respawn_probe /tmp/bri-v022-final-content \
  /tmp/bri-v023-respawn512-burst-gpu.json 'Beta City 16' \
  > /tmp/bri-v023-respawn512-burst-gpu.log 2>&1
```

Both exit 0: all 512 die on frame 10 (0.1667 s), all 512 return on frame 910
(15.1667 s), one host collision refresh per batch, identical observed tick
per batch, all deadline/flags/retained mirror guards pass. Cosmetic bodies
expire together on frame 790 (13.1667 s), separately from restoration.
The actual mechanical definition selected from this content snapshot is
`brick_1randompack:brick/brick1x1fhalfrounddata`, a native 1x1x1 logical
volume with authored convex collision. It is a synthetic focused fixture,
not the user's exact brick mix or a plain box shape. Actual centre
[1.75,379.5,1.75], max box distance2.571959 m, exactly512 sphere candidates,
unchanged gravityrocket radius3/force30/maxvolume30/scale1. The source/save
installation and content packs remain unchanged.

| Stage at the one restoration frame | CPU run ms | GPU run ms |
|---|---:|---:|
| Host two authoritative ticks | 0.956 | 0.844 |
| Replica changed bricks | 0.216 | 0.210 |
| Collision mirror | 0.460 | 0.443 |
| Building query/index update | 0.298 | 0.263 |
| Isolated synchronous mesh costing | 5.322 | 5.281 |
| GPU pooled chunk upload | unmeasured | 0.982 |
| Encode/submit | unmeasured | 0.078 |
| Offscreen completion wait | unmeasured | 0.988 |

Return rebuilt one chunk / 18,432 vertices. Simultaneous cosmetic expiry
costs 1.288 ms CPU / 1.342 ms GPU debris advance, with GPU completion0.285 ms.
Global worst debris advance is the initial 512-body blast:
8.000 ms CPU / 7.738 ms GPU. Peak body/awake count512, contacts1645,
cached statics188. GPU global completion maximum1.194 ms is frame0;
return0.988 ms. Prediction colliders peak1096 and finish584 after restored
geometry; the return retention checks pass. GPU pool stays at two blocks /
22,505,600 bytes for the entire cycle. This is one cycle, not a broad leak test.
The reported huge delayed hitch is not reproduced. The return's dominant
cost is isolated background chunk meshing, and the remaining measured host,
mirror, index and GPU costs do not support a production optimization yet.

Provenance `/tmp/bri-v023-respawn512-burst-provenance.json`: HEAD remains
0eebedb81. Tracked runtime641-file aggregate remained
`6f803004d3a02ca1c301cfacb543f5acbdd4d3fce157ef84482daf31b40600a9`
through compile/retry/CPU/GPU. All-runtime643-file aggregate, including
untracked diagnostic/add-on choices source, remained unchanged during the
valid CPU/GPU runs at
`68180494736f5fefdfdc44a3b10b538e813a5b8f7c1e5a5c2e3363eaa4eae429`.
Probe SHA-256 `4d43ab1610c57fad816dde771ecdf22ac6fa0d8689328d79bb952eba5347659a`.
Executable27,064,576 bytes, SHA-256
`63fe6273adcc255f4600216b331227560c711673cedd6b01659c220a3b115238`;
verified identical CPU/GPU and after GPU. Content aggregate remains the
same9471-file hash recorded above. Current source contains root's earlier
NPC/client integration, so this is a new burst baseline rather than an exact
historical source comparison.

Root identified the next causal gap: continuous destruction leaves newer
live debris when an earlier burst restores. A quiet single burst has no
bodies at15s and cannot exercise generation-triggered Surroundings cache
clear/wake-all. Ordinary High/default body limit512 means a second512 blast
replaces older retained bodies; it does not double retained physics. In this
fixture age3s has512 awake/cache41, age7s494 awake/cache155, age11s0 awake/cache188.
A second burst about12s later exercises restore with active newer bodies;
a second about4s later can isolate waking settled newer bodies. These are
proposed follow-up controls, not run here; no autoexpansion or production
scheduler/budget change occurred. Windows real frametime and user-scale
continuous mayhem remain unverified.

### Paired overlap control prepared, not applied or run

Root authorized preparing `/tmp/bri-v023-respawn-overlap.patch` against the
verified single-burst diagnostic. Shared probe/runtime source remains
unchanged for this preparation. Rustfmt and `git apply --check` pass;
compilation and both paired controls remain unverified pending root's lease.
The exact single-burst executable and source are preserved separately at
`/tmp/bri-v023-respawn512-retained-executable` and
`/tmp/bri-v023-respawn512-retained-source.rs` with the hashes above.

Both cases set `BRI_RESPAWN_OVERLAP=1`, load the same two native512-brick
grids through ordinary LoadBuild, and place the second grid8 m away at the
same height. Both install the same native event rows and verify each unchanged
projectile sphere hits exactly its own512 bricks in the final common world.
A SHA-256 of the complete initial PublicWorld, including palette/all public
brick state, is printed and retained to verify quiet/active world equality.
Both run1,800 frames at the explicit15s respawn setting. The only control
difference is `BRI_RESPAWN_SECOND_BURST_SECONDS=0` (quiet) versus12 (active).
Four seconds is an optional selectable control using the same loaded world.
No explosion, cleanup, event quota, scheduler or cosmetic budget changes.

Quiet must knock/restore its first512 and leave every second-grid brick
intact on every observed frame. Active must knock/restore all1024 in two
strict512 single-tick batches, each on its own native15s deadline; first
restoration must actually have live debris or the interference fixture fails.
Existing flags/collision/index/debris-expiry guards remain. Default cosmetic
limit remains512. Ordinary older-body eviction at a second blast can create
brief fading ghosts (native0.35s); the receipt now counts render instances
as well as physics bodies so that render cleanup is not mistaken for brick
return. Diagnostics add Building query-generation before/after, adaptive
cosmetic room and draw-instance counts outside all stage timers.

Expected active12s profile-relative epochs: first blast0.1667s; second
blast12.1667s; first restoration15.1667s with newer debris age3s; newer native
cleanup25.1667s; second restoration27.1667s. Cap eviction may remove older
bodies at second blast, faithfully following existing policy; that differs
from the single quiet burst's13.1667s expiry. Optional4s mode moves second
blast to4.1667s, first restoration sees newer debris age11s (the prior receipt
shows that age is mostly asleep/cache188), cleanup17.1667s and second return
19.1667s. Analyze event-local stage costs and maxima; shared30s profiling
duration keeps comparison percentiles from changing solely with run length.
The control intentionally changes one ordinary input, so it is a causal
interference comparison, not a production before/after optimization claim.


## Approved paired live-debris comparison — completed

The previously retained overlap patch was applied only to the diagnostic bin.
Root granted one exclusive lease for quiet/12s CPU then GPU controls, no4s or
further scale. Release compilation jobs2 completed with no fixture repair.
Source HEAD was `e262e4553cd3c1ec71189d9ec63531479ae02737` plus recorded dirty
work; this is a fresh controlled comparison, not an exact old512 baseline.
All crate files (including manifests/tests) remained identical through build
and all four executions:1413files, SHA256
`c7c7bf217d750de3848cda7f64d490d7088923c8a8d667ba13d915008d80b5d6`.
Content snapshot9471files/392843334bytes retains SHA256
`161735207b50e5abe574a5b86fca698c9cd969210f74271888a7cd6e98a16a92`
(Path-component sorted relative path+NUL+binary file digest). All four use the
same executable27148880bytes SHA256
`7959b4ec553f5ab892571493a4643621206cdd618ab5c604357b103dc06f876f`.
Exact common initial PublicWorld SHA256
`d15685a11e2553ca664c8d0913022a02f2d29a6f2c3c45dc0ce5a17a385e4075`:
9549totalbricks including both512 grids. Only the ordinary second onActivate
input at12.1667s differs. Native default cosmetic cap512, authored15s return
and13s cosmetic expiry remain unchanged.

Commands after the one compilation:

```sh
env -u CARGO_TARGET_DIR CARGO_BUILD_JOBS=2 \
  cargo build -p bri-client --release --bin respawn_probe

# Run once each with SECOND=0 quiet or12 active; add BRI_RESPAWN_GPU=1
# only for the GPU pair. Redirect each run to its matching retained.log.
env BRI_RESPAWN_MODE=explosion BRI_RESPAWN_BRICKS=512 \
  BRI_RESPAWN_SECONDS=15 BRI_RESPAWN_OVERLAP=1 \
  BRI_RESPAWN_SECOND_BURST_SECONDS=$SECOND \
  target/release/respawn_probe /tmp/bri-v022-final-content \
  /tmp/bri-v023-respawn-overlap-$CONTROL-$ENGINE.json 'Beta City 16'
```

All four exit0 with strict native all-brick, per-wave batch/deadline, flags,
retained collision-mirror and final debris-empty guards. Quiet restores512;
active restores1024 across two512 batches. Quiet verifies its intact second
grid every frame. Active verifies live debris at the first restoration.

| First512 return at frame910/15.1667s | Quiet CPU | Active CPU | Quiet GPU | Active GPU |
| --- | ---: | ---: | ---: | ---: |
| Live bodies/awake before return |0/0|512/512|0/0|512/512|
| Cached statics before return |0|44|0|44|
| Host two ticks ms |.924|.786|.833|.897|
| Collision mirror ms |.460|.442|.428|.453|
| Building query ms |.276|.286|.284|.282|
| Cosmetic advance ms |.0005|1.104|.001|1.145|
| Isolated synchronous chunk mesh ms |10.208|5.382|10.298|5.205|
| Uploaded vertices |36864|18432|36864|18432|
| GPU chunk upload ms |—|—|2.177|1.143|
| GPU completion ms |—|—|1.072|1.312|

Quiet rebuild includes both visible grids, while active has its second grid
hidden until27.1667s; mesh/upload differences therefore cannot be attributed
to a performance improvement from active debris. Both rebuild one chunk.
Active first return advances Building query_generation3→4 and executes the
Surroundings invalidation path with512awake bodies and44cached statics; the
measured cosmetic stage does not show a large stall. Its bodies were already
awake, so this does not isolate a sleeping-body wake-up cost.

Quiet expiry13.1667s costs1.131CPU/1.113GPUms cosmetic advance. Active second
expiry25.1667s costs1.016CPU/1.382GPUms. Active second return27.1667s costs
host.764CPU/.861GPUms, mesh10.031/10.092ms, GPUupload2.260/completion1.562ms.
BothGPUruns pool allocation stays3blocks/69850880bytes for all1800frames;
no return-trigger resource growth. Default cap holds at512live bodies; second
blast temporarily has1024render instances because evicted first-wave bodies
fade as ordinary0.35s ghosts. Peak initial blast contacts1645, quiet cached
statics572, active second wave subsequently164. Full maxima and worst-frame
receipts are retained; largest diagnostic sums occur at initial destruction
~19.6–19.8ms, not at delayed return. These sums are not client frametimes.

No huge delayed hiccup reproduced and no production scheduler/cache/upload
change is supported by this bounded comparison. Mac Metal256x144 excludes
map/avatar/UI/shadows; synchronous meshing measures isolated CPU work unlike
the app background meshing job. Windows rendering, exact user destruction
mix and sustained higher workload remain unmeasured. No Windows lag-fix
claim or threshold weakening. Compute lease explicitly released after four
successful runs; no further optional benchmark expansion.

Receipts `/tmp/bri-v023-respawn-overlap-{quiet,active}-{cpu,gpu}.json` and
matching logs, build log `/tmp/bri-v023-respawn-overlap-build.log`, provenance
`/tmp/bri-v023-respawn-overlap-provenance.json`, exact retained executable/source
`/tmp/bri-v023-respawn-overlap-retained-executable` and
`/tmp/bri-v023-respawn-overlap-retained-source.rs`.
