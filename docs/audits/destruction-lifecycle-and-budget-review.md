# Destruction: conversation, implementation and edge-case cross-check

Reviewed 2026-10-28 at `c9c6d3f64` on
`codex/plastic-destruction-playtest`, PR #379. Comparison base:
`b927779dc2aa9b05552f0c2509fc0a1b5c3f0096`. This records the reviewed
48-file diff, including the preview.2/preview.3 corrections, rather than a
repeat of the original before-state audit.

## What we actually want

Familiar brick destruction with occasional memorable outcomes, using a small
number of well-integrated rules. Preserve believable explosion scale. Make
interesting plausible outcomes accessible; do not amplify every explosion.
Keep the original bricks, details, colours and prints recognizable. Most small
bricks pop whole; eligible direct hits can occasionally produce complementary
shards. Large supported plain boxes split into catalog-sized pieces near a
blast. Remaining material stays real and can be hit again.

Only support actually severed by damage causes a collapse. Foundation slabs
on the map and deliberately floating roots remain anchors; upper slabs and
roofs do not become foundations merely because their supports disappeared.
No weight/stress simulation, crushing damage or replicated moving rubble.

The server owns existence, support, permission and restoration. Clients own
presentation. Visible material, shared animated assemblies and independently
simulated bodies are different resources. Lower quality should reduce
independent motion before losing the recognizable collapse. Rockets continue
through cosmetic rubble for gameplay, but visibly affect what they pass through.
No additional switches or generated sounds.

Later instructions supersede earlier alternatives: no shard toggle, no
server-simulated clumps, no crushing, and no blanket larger throws. Maxwell's
latest priority is coherent lifecycle and affordable large collapses before
secondary polish. "Simple underneath" does not mean treating asynchronous
delivery, edits and restoration as unrelated exceptions.

Evidence: the supplied conversation/drawing, recovery notes, prior implementation
receipts, existing audit/dispositions and the two supplied recordings. The
separate "Review gameplay value" conversation was inspected where accessible;
some assistant messages were opaque content references. No complete independent
Claude transcript was recovered. Its recovered commits and Maxwell's pasted
conversation are the available evidence, not proof that every earlier promise
was implemented. Preview.3's tentative improvement is not merge acceptance.

## The two merge blockers

### A. One authoritative lifecycle, with an explicit presentation handoff

**Code-backed:** `session/destruction.rs:723-850` declares a group's total
membership, starts its clock and hides members in subsequent batches. The client
starts analytic motion on the first member
(`brick_debris/sections.rs:94-158,375-410`). It can land before delivery ends.
Landing breakup waits for `pieces.len() >= expected` (:292-309), but there is
no terminal completion/cancellation record. A deleted member, invalidation or
round cancellation can leave the declared total unreachable. The 13-second
presentation lifetime still expires; metadata retires at 30 seconds.

Normal late delivery was improved: landed groups now admit later members. That
closed the old unconditional landed-member rejection. It did not establish a
shared transaction with a known terminal state. Keeping those two facts separate
matters.

Tools, events and restoration already have a useful common boundary in
`simulation/damage.rs`. Family ownership preserves originals and deliberate
part edits commit remaining geometry. However, section scheduling lives separately
in Session, and membership is not a first-class operation at that boundary.
Server invalidation can abandon the remainder of an old group and start another
proof without telling clients how the old group ended. Each removal batch also
starts its own original respawn timer (`destruction.rs:405-423`).

**Cross-check:** a skyscraper is still being delivered when its base is restored;
an event deletes a queued member; a builder edits a surviving partition; a
minigame resets halfway through removal; two rockets affect overlapping groups.
These are lifecycle transitions, not separate landing-animation bugs.

**Required correction:** define an authoritative, versioned section operation
with stable source membership and explicit prepare, commit, cancel and retire
transitions. World mutation, selection/copy/undo, damage, save projection,
replication and client presentation must consult the same ownership/state.
The presentation may lag to prepare resources; it must not infer completion
from elapsed time or an expected count alone.

- Prepare membership and necessary derived data incrementally against a revision.
  An edit that invalidates preparation restarts/cancels that operation explicitly.
- Commit has one documented gameplay meaning. Do not silently call a long series
  of partially removed ordinary bricks an atomic detached section.
- Publish bounded membership records and a terminal revision/barrier. Start the
  coherent fall only when the client has a usable representation of the committed
  operation. Slow presentation must not hold the server waiting for GPU uploads.
- Restoration/reset/delete terminates or supersedes the operation explicitly;
  stale members cannot resurrect old presentation or hide restored bricks.
- Collision/query adapters must obey the logical state too. Adding a section
  enum only to the renderer would leave the architectural problem intact.

This needs a defined consistency model, not another time delay or an "end"
message pasted onto the existing animation.

### B. Admission and budgets for the whole pipeline

**Code-backed:** there are useful limits, but they measure different things.
Server support work is resumable; body limits are real. Neither bounds every
large operation:

| Stage | Present mechanism | Remaining exposure |
| --- | --- | --- |
| Server damage admission | 256 pending blasts | Not a bound on total search/component membership or all derived allocations |
| Support queries | 8,192 counted units/tick | Watched-region invalidation/restart scans are outside that counter |
| Support scheduling | Lowest-support-first traversal | A search is pushed back to the front; a large proof can delay independent proofs |
| Mutations | 128 candidate commits/removals per tick | One split can insert up to 2,048 cells; 128 candidates is not 128 world/network mutations |
| Restoration | 128 due originals/tick | Each original can own many parts; restoring families removes those parts synchronously |
| Section delivery | Up to eight proved groups share removal rounds | Brick deltas and per-member cues still scale with material affected |
| Client admission | 65,536 reserved visible members; 1,024 group records | An oversized next group is rejected wholesale, so the new collapse can disappear |
| Contact queries | 4,096 work units/frame | Displayed fall time waits for query completion; old groups consume budget first |
| Landing partition | 4,096 growth units/frame | Final member scans, proxy sorting and compound construction run after planning in one call |
| Chunk simulation | 64 active group bodies; 2,048 proxy primitives, shared with ordinary debris | A chunk needing more than 64 proxy boxes gets no body; exhausted slots freeze art rather than coarsen it |
| Nearby collision | 512 preparation units/frame for chunks | Global query-generation changes restart preparation; ordinary debris loading and cache scans are separate |
| Render preparation | Shared palette and per-look instancing | All missing live looks build/upload that frame; the 64-look cache keeps extra live looks |
| Render update | One instance per visible member | Grouped physics does not remove per-member CPU transforms, instance uploads or GPU geometry cost |
| Cue queues | 4,096, with reported drop counts | Overflow is detectable, but lost section members have no section reconciliation protocol |

References: `session/destruction.rs:12-16,128-145,493-545,587-854`;
`simulation/damage.rs:78-118`; `session/events.rs:837-848,941-982`;
`brick_debris/sections.rs:292-343,382-529`;
`sections/proxy.rs:5-62`; `brick_debris.rs:889-913,1069-1128`;
`local_physics.rs:124-164,168-231`; `presentation.rs:525-545`;
`net/replica.rs:375-413`.

**Cross-check:** a dense skyscraper, a sparse intricate mast, repeated blasts
across distant builds, many different prints/colours, a respawn storm, and two
collapses overlapping an existing pile stress different resources. A 40-body
counter can look excellent while those paths remain expensive.

**Required correction:** admit the operation before destructive commitment,
account separately for source material, staging memory, traversal, mutation,
proxy complexity, render construction/upload and transport backlog. Make costly
construction resumable, including finalization, with fair scheduling. Use
connected coarser motion groups and shared render data first; preserve holes and
source geometry. A larger bounding cube is not an acceptable representation of
the sails.

When a hard visual/resource limit is reached, retain/defer a coherent source
representation while preparing an affordable presentation. Do not remove the
whole structure and then discover its visual operation cannot be admitted.
This policy must share blocker A's lifecycle. Raising 65,536 or hiding overflow
pieces would merely move the failure.

Rendering all detail of an arbitrary million-brick collapse cannot be guaranteed
at fixed cost. The explicit fallback must preserve its recognizable material and
silhouette with cheaper motion/render detail, or defer the operation coherently.
That limitation belongs in the design and handoff, not behind a body-count claim.

## Symptoms explained by these boundaries

These are code-supported failure paths, not a claim that a recording identifies
one exclusive cause.

1. **Smooth game, jerky destruction.** Section `shown` time only advances after
   all pending contact probes finish (:245-289). After landing, region preparation
   resets on global `Building::query_generation` or bounds expansion and calls
   `set_enabled(c.ready)` (:475-496). Work starvation can hold the same pose,
   then advance it abruptly. Remote geometry changes can repeatedly invalidate
   unrelated chunks. The fixed-step accumulator also caps/drops work. More
   rigid bodies or a higher frame cap would not correct these dependencies.
2. **Visible rubble without a hit target.** Analytic assemblies, proxy failures,
   not-yet-admitted chunks and budget-frozen chunks have no active body. Shots
   query only dynamic-body colliders (`local_physics.rs:468-516`). Keeping
   sleeping bodies, as preview.3 does, fixes only the sleeping case.
3. **Visible rocket and hit ray can disagree.** The renderer uses the coasting,
   muzzle-corrected `shown` view (`app/frame.rs:1048-1079`), but rubble hits
   use raw `view.weapons` (:2003-2010). `Shots::strike` omits the first observed
   segment and drops a disappeared projectile without sweeping its last segment.
   `fired()` is the live-projectile iterator; its name is not the bug.
4. **A registered hit can remain subtle.** The chunk impulse floor is mass times
   1.5 (`brick_debris.rs:804-839`). The existing test proves velocity/waking,
   not that Maxwell will notice the response. First establish correct visual
   path and target coverage, then tune a restrained local reaction.

The correction should separate presentation and hit representation from admission
of independent simulation bodies. Collision preparation must not repeatedly
disable otherwise visible wreckage because unrelated world geometry changed.

## Secondary issues to keep attached to the main work

- **Permission policy diverges by entry point.** Queued Scope permits every LAN
  brick outside a minigame (`destruction.rs:54-58`), while
  `events.rs:1095-1117` now applies ownership there. Both cannot be the common
  policy. The supplied conversation requested own bricks outside minigames;
  earlier source behavior allowed LAN exceptions. Resolve once at the shared
  damage policy, record the intended departure and apply it to all damage
  providers. The synthetic permission check at `destruction/tests.rs:186-192`
  asserts immediately after queueing, without advancing the queue, so that
  assertion alone cannot demonstrate denial.
- **Admission policy is frozen, but round validity is live.** Scope snapshots
  ownership/rules, and only game existence/round invalidates it. Shooter
  disconnect is intentionally covered. Host changes to damage permission,
  membership or trust during a long operation need an explicit rule; do not
  accidentally mix current and admitted policy.
- **Contact geometry is an approximation.** Seventy-two sampled material points,
  straight sweeps between rotated poses and box-union landing proxies cannot
  prove exact rotating-surface contact. Client face-adjacent partitioning also
  differs from authoritative authored stud connections. Keep those cosmetic;
  do not introduce content-name special cases or a structural solver.
- **Caches and lifetime need owners.** Split palettes invalidate by definition
  count, not definition revision. Collision statics accumulate as new regions
  are visited, and global-generation synchronization scans the cache. These
  costs must be owned by the new resource/lifecycle rules rather than assumed
  to disappear because bodies sleep.

## Edge-case cross-check matrix

This is a reasoning/acceptance inventory, not a claim that every row was run.
"Existing check" means source coverage was found; prior receipts retain their
actual test results. No tests or new scenarios were executed for this review.
A/B identify the merge blockers above; S identifies secondary policy/coverage.

| # | Situation | Required invariant / present assessment |
| --- | --- | --- |
| 1 | Rocket into ordinary small-brick wall | Existing weapon eligibility; original bricks pop; rare eligible cracks, no radius amplification. Existing damage/debris checks. |
| 2 | Rocket grazes edge of a 64x64 block | Exact catalog partition; reached eligible cells go, rest remain real. Existing integrated volume check. |
| 3 | Direct hit and radial phase reach same source | No duplicate removed volume, events or family registration. Shared operation must deduplicate. A |
| 4 | Second rocket hits a surviving partition | Descendant changes stay in the original family until deliberately committed. Existing repeated-hit check. |
| 5 | Blast catches two neighbouring large blocks | Total split allocation, not a fresh unlimited allowance per source. Existing tick/blast limits; mutation cost still B. |
| 6 | Floor has wall bricks standing across cut line | Prefer nearby authored edges; never invent a claim that all cuts avoid walls. Existing planner rule. |
| 7 | Hollow arch, ramp, pole, round brick or special brick | Do not replace authored shape with real box partitions. Existing plain-box eligibility checks. |
| 8 | Assigned print or unavailable replacement sizes | Keep original under oversized unsupported/capacity fallback; preserve identity. Existing planner admission. |
| 9 | Transparent paint crosses volume threshold | Apply weapon's existing effective-volume rule consistently; actual geometric material is conserved. Existing branch; include threshold boundary. |
| 10 | Foundation baseplate on terrain/interior ground | Protected by catalog role and real contact, not every large ground block. Existing foundation check. |
| 11 | Elevated baseplate floor or roof | Breakable; last support does not turn it into a floating root. Existing upper-slab check. |
| 12 | Ordinary large cube on ground | Can split even if catalog category includes Baseplates. Existing regression. |
| 13 | Deliberately force-planted floating build | Existing root remains anchored; unrelated blast does not drop it. Existing regression. |
| 14 | Slab remains attached through one narrow connection | Stay until final live connection is cut; no arbitrary blast-radius collapse. Existing support case. |
| 15 | Split source has surviving side faces touching | Internal material continuity survives clean partitioning. Existing seam regression. |
| 16 | Bridge attached at both ends, then one end | Still connected until actual graph disconnect; cosmetic span motion after commit only. A |
| 17 | Roof supported from above or off to the side | Traverse authored connectivity, not just a vertical support stack. Source-based graph rule. |
| 18 | Foreign-owned/indestructible neighbour anchors a section | Damage cannot pull protected material out indirectly; common policy. Existing traversal boundary; S |
| 19 | Shoot pirate mast at broken base | Whole recognizable mast/sails first, then bounded assemblies; no premature base-probe landing. Existing shape test; sampled-contact approximation remains. |
| 20 | Broad sail crosses narrow deck | Material samples, not centre only; no solid bounding box across empty space. Existing contact/union checks. |
| 21 | Wide floor drops onto surviving columns | Floor motion/contact respects committed state; sparse witnesses are cosmetic approximation. A |
| 22 | Long bridge rotates against building side | Side impact can terminate fall; no floor-only contact rule. Current correction; rotating sweep approximation. |
| 23 | Two towers disconnected by one blast | Independent proofs/removals/presentation get fair work, not only eight-group commit fairness. A/B |
| 24 | Giant proof touches unrelated nearby builds | Traverse only connected candidates; bound invalidation work and accumulated search memory. B |
| 25 | World changes during a resumed support proof | Revision invalidation before commit; no stale support conclusion. Existing spatial invalidation, cost/fairness B. |
| 26 | Player plants a new support during preparation | Cancel/revalidate before commitment; all consumers observe the same outcome. A |
| 27 | Builder paints/edits a surviving part | Deliberate geometry commit preserves edit and cancels original restoration. Existing family mutation checks. |
| 28 | Undo/delete targets original with many descendants | Retire all children and dirty state once; work proportional to parts is accounted. Existing family delete check; B |
| 29 | Event deletes a member not yet delivered | Explicit changed membership/cancel terminal state; never wait forever for old count. A |
| 30 | Event restores a hidden original mid-operation | Supersede old operation; stale cues cannot hide/duplicate the live source. Existing flag guard is only part of A. |
| 31 | Minigame reset during preparation/removal/fall | Cancel by round revision, restore originals, explicitly retire old presentation. A |
| 32 | Very short respawn while tail is still detaching | Define operation lifetime; no contradictory removal/restoration under one group. A |
| 33 | Save in each phase, then reload | Canonical originals until deliberate geometry edit; no transient children or hidden damage flags in normal save. Existing save checks plus A. |
| 34 | Copy/stack/box selection during partial destruction | Visible authored geometry only; pending/committed ownership is respected. Existing hidden filtering plus A. |
| 35 | Original invisible/raycast/collision flag combination | Exact original flags return; destruction is distinct from intentional invisibility. Existing regression. |
| 36 | Player/vehicle occupies respawn volume | Preserve existing gameplay overlap behavior; cosmetic pile does not block the server. Existing contract, not a new crushing rule. |
| 37 | Shooter disconnects after admitted hit | Stable admitted attribution/ownership; no orphan work. Existing targeted regression. |
| 38 | Host changes damage permission during queued work | One explicit admitted-vs-current policy across providers. S |
| 39 | LAN / internet / imported-owner aliases / minigame boundary | Same permission function at every entry point; test after queue execution. S |
| 40 | Late join during preparation, fall or restoration | Authoritative live world correct without historical cosmetic replay; versioned state prevents ghosts. A |
| 41 | Cue arrives before brick delta, or delta before cue | Ordered barrier/revision reconciles both; don't infer existence from animation. Existing timestamp guard plus A. |
| 42 | Duplicate or delayed member after landing | At most one source representation; completion no longer inferred from clock. Existing normal-late regression plus A. |
| 43 | Member never arrives / operation is superseded | Bounded explicit recovery or cancellation; no permanently incomplete assembly. A |
| 44 | Network/cue backlog overflows | Preserve authoritative correctness, diagnose loss, reconcile section presentation; no disconnect cascade from an uncontrolled burst. A/B |
| 45 | Low/Off/Best quality, changed during collapse | Lower independent motion first; do not delete member detail merely to meet body count. Existing preservation direction; B |
| 46 | Existing pile consumes body/proxy slots | New operation still has coherent presentation and hit coverage; no unexplained frozen/missing new mast. A/B |
| 47 | Sparse 100-brick assembly exceeds 64 proxy boxes | Coarsen/repartition within bounded work without inventing solid air; do not silently leave inert art. B |
| 48 | More than 65,536 visual members | Admit/defer or use cheaper coherent representation; never remove then discard entire new collapse. A/B |
| 49 | Thousands of unique colours/prints/shards | Bound mesh generation/uploads and retained GPU resources separately from bodies. B |
| 50 | Repeated distant blasts with old rubble alive | Bound collision-cache retention, global invalidation cost and fair progress. B |
| 51 | Respawn storm across split originals | Count descendant retirements and network work, not just due-original count. B |
| 52 | Rocket hits sleeping wreckage | Retain query target and wake it. Existing preview.3 regression; perceptual response still unaccepted. |
| 53 | Rocket hits disabled, frozen or pre-body wreckage | Hit representation remains available without buying every piece a dynamic body. A/B |
| 54 | Rocket spawns inside/near pile, ends next frame | Sweep birth/terminal segments on the displayed path; no missing first/last interval. A |
| 55 | Rocket is muzzle-corrected, coasting or vehicle-attached | Hit path and drawn path agree; server gameplay trajectory unchanged. A |
| 56 | Frame hitch, high FPS, pause/time scale or loading | Presentation clocks, work budgets and fixed-step catch-up are explicit; no repeated freeze/jump or instant TTL loss. A/B |
| 57 | Rubble passes portal or player/vehicle pushes it | Local behavior only; no invisible authoritative wall/damage. Existing ordinary mechanisms; initial analytic assembly limitations explicit. |
| 58 | Second blast hits mid-fall or during breakup | Stable operation/assembly identity; response does not restart membership or double-remove originals. A |
| 59 | Reset/map change/disconnect with pending queries | Release retained source/proxy/GPU/query state; old generation cannot affect new world. A/B |
| 60 | Different mod namespace, same-size brick or hot reload | No arbitrary replacement from another package; invalidate catalog cache by content identity/revision. Existing namespace rule; S |

## What remains worth keeping

The server/client authority split, weapon-derived size rules, conservative
unsupported/capacity fallback, family ownership at Simulation, exact plain-box
partitioning, complementary shards, palette sharing, source-shape drawing and
connected chunk direction are sound foundations. This review is not a request
to replace them with a general structural physics simulator.

Existing checks cover exact partition volume, repeated damage/save/respawn,
upper/foundation/floating cases, family edits/deletes, normal late delivery,
connected material, basic mast motion, gaps in collision proxies and sleeping
hit targets. They do not prove end-to-end lifecycle coherence, bounded total
cost, perceptually convincing hits or arbitrary skyscraper performance.

## Decision and next increment

Keep the design direction. Treat A and B as merge blockers and implement them
together: shared operation ownership and resource admission before destructive
commitment. Move heavy derivation into fair, resumable preparation and make
completion/cancellation observable. Then correct displayed-path hit coverage
and tune the local response through that representation.

Do not start with stronger impulses, longer lifetimes, larger hard caps or
per-map mast exceptions. Do not claim a new playtest or merge readiness from this
inspection. The full integration gate remains incomplete, and Maxwell's actual
playtest acceptance remains outstanding.

This pass used code/diff reading, existing evidence and thought experiments.
No new game session, headless scenario suite, benchmark or build was run.
The earlier audit remains historical; this document is the current prioritized
cross-check and does not reopen every previously corrected finding.


## Implementation disposition

The subsequent hardening replaces these reviewed paths with a common Simulation
section journal, explicit actual-count Ready/Retired barriers, tagged source
recovery, pre-removal presentation reservations, descendant-weighted restoration,
coarser shared fallback and staged client work. See
[the current design](../architecture/brick-destruction.md) and
[implementation evidence](../progress/2026-10-28-destruction-lifecycle-hardening.md).
The line references and blocker descriptions above describe the review snapshot,
not an assertion that all those paths remain unfixed. Residual limits are explicit:
streamed gameplay commits, conservative extreme-size holds, approximate cosmetic
collision and incomplete terminal-only projectile sampling.
