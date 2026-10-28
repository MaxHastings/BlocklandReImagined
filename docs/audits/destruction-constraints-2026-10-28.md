# Destruction constraints and fidelity audit — 2026-10-28

Maxwell requested a full read-through of constraints/optimizations in the
destruction experience, including server authority, delivery, client motion,
rendering and their dependencies. Scope is this destruction pipeline, not an
unrelated whole-engine rewrite. This is a code audit and correction ledger;
pending checks below are not playtest acceptance or measured improvements.

## Experience to preserve

Ordinary small bricks keep their recognizable throws. Large plain bricks split
on studs; a removed region and its debris have matching volume and appearance.
Real remaining bricks stay authoritative, originals save/respawn whole, and only
newly disconnected sections fall. Foundation baseplates and authored floating
roots stay anchored; upper baseplates are ordinary slabs. A complete mast/sail
assembly remains visible while falling, then uses connected physics groups.
Physics groups are not replacement artwork. Cosmetic motion does not crush
players or block authoritative rockets; player/vehicle/projectile reactions
remain visible. Keep the recorded sound tuning Maxwell accepted (+6 dB), with
existing positional variation and bounded starts. No new fidelity switch.

## Coverage and dispositions

The trace starts at session/debris.rs and session/destruction.rs, through
simulation/support_probe.rs, simulation/sections.rs, session/fracture.rs,
presentation.rs, net/protocol.rs and client/network.rs/app/net_events.rs, then
client/brick_debris.rs, all brick_debris/sections modules, local_physics.rs,
building.rs, brick_shards.rs, brick_fade.rs, foley.rs and app/render.rs.

| Constraint / optimization | Effect on the experience | Disposition / reason |
|---|---|---|
| Foundation, floating-root, indestructible and permission anchors | Some bricks intentionally remain supported/protected | Keep authored/game rules; a visually floating remnant alone does not establish a bug in this rule. |
| Weapon size limit / plain-box catalog / printed-brick exclusions | Big ordinary boxes split only into available legitimate pieces; custom shapes/prints cannot safely be substituted | Keep exact representability; record unsupported shapes instead of inventing art, collision or extra volume. |
| Split near blast, coarse valid cells outside blast, neighbor-edge snap | Avoids fragmenting an entire floor into tiny pieces | Keep; standing cells can exceed weapon limit outside the reached region. The conversation's earlier claim that every remaining cell must fit the weapon limit was too broad. |
| 2048 pieces per blast/family, 16384 live split parts, world brick cap | A truly oversized/unrepresentable transaction leaves its source whole | Keep hard memory/world validity ceilings; distinguish from temporary tick allowance below. Never turn a failed split into removal of the whole large brick. |
| 2048 new pieces per tick | Previously could consume a valid hit without doing damage when earlier hits used that tick's allowance | **Correct:** plan against real capacity; defer an untouched hit with its support proof when this tick lacks room. |
| 4 MiB mutation bytes per tick; cue headroom of 16 | Previously could discard a representable split because of work already done this tick | **Correct:** retry temporary shortages. Reject only a transaction that cannot fit an entire fresh tick. No partial mutation before retry. |
| 8192 support/query credits, 256 commits per tick, eight committing groups | Spreads real authority/index work; large structures can wait for proof and complete membership | Keep resumable work and independent groups. Validate small-structure latency and no starvation; do not increase every quota to hide unrelated client physics cost. |
| 256 queued jobs / retained proof-member bound | Excess admitted work can be refused, leaving sources standing | Keep finite authority/memory admission; report as refusal, not a successful explosion. Future overload tuning must preserve the admitted-job lifecycle. |
| 65536 support neighbors / section members | Over-envelope proof does not remove unsupported material without a representation | Keep safety, explicitly acknowledge held material; this is not evidence that Maxwell's small scene hit it. |
| Regional proof invalidation and interrupted-section retry | Relevant edits invalidate a support claim while another hit/build/restore occurs | Keep correctness. Repeated nearby edits can delay a proof; test shared lifecycle rather than bypassing it. |
| 64 historical section reservations, retained 3600 ticks | Could hold the 65th tiny collapse standing despite spare material/render capacity | **Correct:** use the existing client metadata envelope (1024) as a shared constant. Keep separate material/geometry quotas and commit-rate bounds. |
| Presentation member/look/quad/bucket reservations (65536 / 256 / 65536 / 262144; draw quads 524288) | Can defer new collapses rather than silently hide members | Keep actual renderer/resource envelopes. A count of groups is not a substitute for these costs. Repeated looks are conservatively charged per reservation; not a claim of exact unique GPU cost. |
| Lease / journal retention / retirement barriers | Retains terminal knowledge across delivery, respawn and restoration | Keep; shortening a lease solely to free a counter could recreate missing late members. |
| Protocol movement/world update intervals; event queues | Delivery is batched; client presentation follows authoritative membership/terminal barriers | Keep ordering/recovery. A long `network` span includes event handling and is not proof of Internet latency or bot CPU. |
| Sealed complete section / recovery quota 256 / 30-second stale staging | Avoids launching an incomplete structure, then rejecting late pieces | Keep completion and journal recovery. Normal small sections must start promptly; expired staging is a fault safeguard, not a supported visual fallback. |
| GPU-readiness gate and four new look builds per frame | Whole assembly waits for its actual visuals; ordinary new-look debris can appear late | Retain bounded expensive model construction for now: prior source evidence records large upload spikes. This is a real fidelity tradeoff, not free optimization. Check missing-look/start latency in the final candidate. |
| 64 cached looks | Evicts only looks unused this frame; active looks above 64 remain | Keep cache reuse; this is not a 64-visible-look cutoff. MAX_INSTANCES includes all admitted section members. |
| Physics Quality loose-body count / oldest 0.35-second ghosts / kill-pop cap | Ordinary excess loose debris fades, potentially skips if never displayed | Keep existing v20 quality behavior. Section members do not consume one body slot per brick. Do not silently apply this loose-body policy to a complete collapse. |
| 64 connected chunk bodies; 2048 total proxy parts; quality grouping | Reduces independent motion without changing original visible brick geometry | Keep bounded physics complexity. Current fallback retains the coherent landed assembly; this visibly reduces breakup and must not be described as identical high-quality physics. |
| Connected partition target, 4096 planning credits, 256 indexed members/frame | Planning may finish during flight; real member/index work is amortized | Keep incremental planning and fair section rotation. No dependence on cosmetic brick names. |
| Exact rectangular proxy merging, 64 parts per chunk, no invented bounding hull | Preserves empty sail/beam space; over-complex proxy keeps shared assembly | Keep shape fidelity; audit shows the fallback is a loss of breakup, not of drawn material. Do not replace it with giant solid cubes or charge each visible brick as a body. |
| 72 landing witnesses; 4096 probe credits; 0.2-second look-ahead | Cheap shape-aware landing; incomplete collision work can limit motion progress | Keep confirmed collision safety; audit records possible stalls under pathological invalidation, not measured as the cause of the supplied scene. Check ordinary/large mast movement and first contact. |
| Shape-aware drop/topple/span and pivot release, angular/speed clamps | Deliberate cosmetic animation rather than full structural dynamics | Keep scale-aware motion and measured throws. Not a reason to delay the whole collapse or inflate blasts. |
| Collision windows, 128*16 loose and 512*16 chunk query credits | A body is paused if it cannot safely move through prepared geometry | Keep cached ready windows and fair turns. Verify that ordinary hits promptly prepare collision; no lowering these limits as a frame-rate fix. |
| 8-unit cached tiles, 16384 static/cached-tile ceiling | Historical cache growth can block a new region; reset requires all bodies disabled | **Open risk:** extreme saturation can leave some motion waiting. Not identified in supplied logs (which lacked static counts). Add static/awake counts to future slow-frame diagnostics; evaluate a cache-lifecycle correction rather than lifting the ceiling blindly. |
| Global wakes on any regional edit, changed support and cache clear | Restarted unrelated settled piles and compounded catch-up costs | **Correct:** remove global wakes; Rapier wakes removed-collider contacts, and insertion uses its local broad phase to wake overlapping sleepers. Map/support removal must still wake affected bodies. |
| Sleeping bodies skipped when collision window unchanged | Keeps settled art and projectile targets while removing solver work | Keep; exact pose remains drawn and normal impulses wake the body. No irreversible removal of sleeping debris. |
| 120 Hz, maximum four catch-up steps, two cosmetic solver iterations, CCD | Four-step frames multiply cost; dropping time avoids unbounded backlog but can slow apparent motion under sustained load | Keep timely fixed motion and existing collision reliability while targeting unnecessary active work. Do not lower step count or physics rate to claim a performance win. Two iterations remain a declared cosmetic approximation, not structural simulation. |
| Player/vehicle reach 6, first 32 eligible pushers in ID order; teleport threshold 5 | Distant actors excluded; extreme crowds may not all push debris | Keep locality/finite work. Rendering/authoritative actors remain unchanged. Nearest selection is not guaranteed by the current ID sort, so do not claim it is. |
| Projectile segment max 100; disabled-body casts 512; section queries 1024 admission / 512 steps / 256 pending | Cosmetic hits can be delayed/missed under extreme demand, without changing authoritative shots | Keep bounded queries and source-box gap checks; test ordinary and shared/sleeping targets. No collision proxy means a source-box reaction still exists. |
| 10-second solid + 3-second fade; frame-time expiry | Debris eventually clears; physics debt does not extend its lifetime | Keep existing lifecycle. Expiry does not justify hiding members on initial admission. |
| Three shard planes / at most eight pieces / direct-hit chance 1/4 | Deliberate variety; same source volume and materials | Keep exact partitions and current restrained tuning, not larger explosions or synthetic sound. |
| Fade cache 512 / geometry reuse within material blend class | Excess simultaneous legacy fades may snap; caching avoids repeated geometry builds | Keep measured geometry reuse. Cache-bound overflow remains a known visual tradeoff, not the dominant recurring physics cost in the new logs. |
| Foley 64 reports/frame, history 4096, 8 starts/sec burst 3, source/cell cooldowns and radii | Selects audible contacts instead of a sound per brick | Keep tasteful finite audio; +6 dB changes sample gain, not event count or simulation. |
| Legacy killBrick/fakeKill chain cue sampling at 2048 | Large old-style wand/command removals may not draw every removed brick | Existing legacy fidelity/quality policy, separate from admitted structural sections. Do not represent it as volume-preserving full-collapse presentation. |

## Evidence and pending completion

Preview 11 session-20261028-211351 records six detailed gameplay hitches with
1195–1683 loose bodies, 55–64 chunks, four steps, 25.8–36.8 ms debris work and
20.1–33.9 ms simulation; network 0.5–1.4 ms. The previous session also contains
an 84.8 ms client network/event-handling hitch and a 10.8 ms fade upload. They
are separate costs. Logs do not prove that every active body was legitimately
moving, nor identify a specific constraint as the cause of the reported float.

The new support regression fails on the old code when removal of one support
wakes unrelated rubble. With local waking it passes removed-support drop,
unrelated sleep, paint stability and new-solid overlap displacement. A first
version used an off-grid dynamic resting pose as a planted-brick position;
the corrected fixture inserts at its authored stud/plate position.

The new 65th-small-section admission, temporary split retries, local support/
sleep regression and existing debris/section/Foley/Add-On physics checks pass.
The wake comparison did not establish a useful performance gain. Native match
profiling reproduced the dense-contact cost; the subsequent parallel experiment
did not establish a convincing integrated win and is excluded from the merge.
Maxwell narrowed this task to merging the verified fixes, then requested one
combined playtest with the other active lane. Final scoped checks/platform CI
and packaged startup are recorded in the progress handoff, not assumed here.
The extreme saturation/start-latency risks and dense-contact cost above remain
explicit; this audit is not a zero-risk or FPS-fix claim.
