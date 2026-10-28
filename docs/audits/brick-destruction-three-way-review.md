# Brick destruction: conversation, implementation and edge cases

2026-10-27. Inspection checkpoint on `codex/plastic-destruction-playtest`.
This reviews Claude's recovered commits, the current integration diff including
new files, and the existing systems they call. It is a design/code review,
not a claim that scenario runs or screenshots establish readiness. New
real-content scenario work is held while this review is resolved.

## What Maxwell actually wants

Familiar building-game destruction that remains enjoyable after hundreds of
shots. Simple rules should produce an apparently rich experience cheaply.
Increase access to uncommon believable outcomes, not the size/strength of
every explosion. Keep the original art, scale, colors and sounds. Tasteful
variation is welcome; manufactured audio, arbitrary material, visual spam and
an expensive general structural simulator are not.

The latest directions resolve the conversation's abandoned alternatives:

1. Whole small bricks remain the common outcome. Occasional direct-hit cracks
   are complementary parts of the destroyed brick, not extra decoration.
2. Big ordinary boxes can receive stud-aligned bites. Surviving partitions are
   real solid bricks. Fine near the impact, coarse away. The weapon's existing
   damage limits remain the basis; no blanket increase to radius or force.
3. A foundation baseplate resting on the map is protected. A second-floor or
   roof baseplate is destructible. Ordinary large ground blocks are destructible.
   Deliberately floating roots remain anchors. Removing supports never promotes
   a roof into a new foundation.
4. Only support actually severed by destruction causes a section to fall.
   Remaining connections hold it up. No weight/stress solver or forced random
   collapse to manufacture spectacle.
5. Server authority changes existence/support. Clients own all debris motion.
   Falling clumps break into their own constituent bricks on landing, cannot
   crush players and never become new permanent rubble.
6. Originals own saves, respawn and authored events. Descendants do not multiply
   those identities. A second hit and a mid-fight save must work normally.
7. Full experience enabled, no new feature toggles. Existing damage permissions
   and Physics Quality still apply. Free-build changes stay within the agreed
   ownership rules and use the existing respawn policy.

Two earlier claims need precise limits: no *motion stream* does not mean zero
network cost; additions/removals and cues grow with affected bricks. Likewise,
proving that a large connected section has no anchor can require examining that
section. The achievable rule is no unrelated-world flood and bounded scheduled
work, not an invariably tiny fixed-radius query.

## How the current pieces fit

Weapon collision/explosion -> `Session::blow_up_bricks` -> damage permissions
and native volume rules -> stud partition planning -> authoritative mutations
and original/fragment bookkeeping -> deferred support search -> death cues and
ordinary world deltas -> client Building/query/chunk mirrors -> local debris
and GPU models. Respawn, reset, events, saves, undo and administrative removals
must all reconcile the same original/fragment family.

| Part | Relevant source and inspected integration |
|---|---|
| Blast policy and direct/radial phases | `crates/sim/src/session/events.rs`, `weapons.rs`, `movables.rs`; native volume, transparency, permission and respawn paths |
| Geometry partitioning | `session/fracture.rs`, `definitions.rs`, `grid.rs`; catalog selection, cell cuts, colors/flags, stud connections |
| Existence/support | `session/destruction.rs`, `simulation.rs`; fragment/original maps, roots, search frontiers, collision refresh and world budgets |
| Other authoritative writers | `session/debris.rs`, `events.rs`, `admin.rs`, `admin_world.rs`, `copy_edits.rs`, `copy_jobs.rs`, `undo/jobs.rs`, `build_load.rs`, `packages.rs` |
| Save/reset/map lifetime | `Session::saved_world`, `revive_game_bricks`, `respawn_bricks`, `map_change.rs`; a fresh map gets a fresh destruction runtime |
| Replication | `crates/net/src/server.rs`, `protocol.rs`, `replica.rs`, `codec.rs`; snapshots, bounded cue queue, reliable update backlog, cursor validation |
| Client integration | `app/frame.rs`, `app/fx.rs`, `building.rs`, `world_chunks.rs`, `brick_fades.rs`; hides, tool ghosts, queries, incremental geometry |
| Local presentation | `brick_debris.rs`, `brick_debris/sections.rs`, `brick_shards.rs`, `local_physics.rs`, `audio.rs`; contacts, budgets, models, existing break-sound rate limit |

## Findings visible from code

These include verified control-flow gaps and unresolved policy/performance risks,
not 25 reproduced failures. Each row gives its code basis and exposing case.
Resolve it through a fix or a documented supported policy before handoff; do
not mark it closed merely because an unrelated scenario passes.

| ID | Requirement × code × exposing case | Consequence / correction |
|---|---|---|
| R25 | Remaining slab stays held by its walls × partitions become independent bricks, while `connected_bricks` only follows up/down studs × small surviving floor cells beside a hole have no direct wall contact | The act of representing a slab as partitions can sever intact material outside the blast and enlarge the collapse. Preserve the original family's material continuity across adjoining surviving cells; do not invent side bonds between unrelated bricks. This is a core topology gap, even in an ordinary floor scene. |
| R1 | Cheap support × `SUPPORT_WORK` counts vertices, while `rests_on_map` calls unbounded `ground_probe` over every footprint cell × wide slabs or a skyscraper | A small vertex budget can hide thousands of raycasts. Charge actual cells, candidates and connections; retain cursors within expensive queries. |
| R2 | Eventual local support resolution × one global `support_epoch` invalidates a whole search × unrelated building or sustained nearby shots | Large searches can repeatedly restart and starve. Invalidate affected proofs/regions, coalesce work, and retain progress under unrelated edits. |
| R3 | Cheap mass respawn × `respawn_bricks` restores all flags, then calls `mutate_many` once per recorded original × thousands of collapsed originals | Can trigger one collision refresh per brick. Restore final flags in one batch, then fire original inputs once. |
| R4 | One family lifecycle × direct removals in admin clear, duplicator cut and undo bypass `clear_damage_pieces` × cutting/removing the hidden original mid-fight | Orphan descendants, stale timers/metadata, wrong saved/restored result. Centralize family reconciliation across every writer rather than patching just weapon paths. |
| R5 | Originals return once × event `Presence(revive_fake_dead)` removes the timer and independently unhides flags × onBlownUp immediately restores a split original | Original and surviving fragments can coexist. Event unhide/fake-kill/respawn/delete must use the same transition boundary. |
| R6 | Preserve original flags × `Cell::brick` uses default visible/raycast/collision flags × an invisible colliding or ray-disabled ordinary box splits | Can create visible or ray-blocking material that was absent. Copy the relevant flags; keep original-only properties/events singular. |
| R7 | Structural existence distinct from collision × `live_damage_brick` uses `colliding`, but the existing stud graph ignores that flag × an authored non-colliding support | New blast support semantics can disagree with tools/building. Define support liveness explicitly; distinguish fake-dead originals from intentionally non-colliding living bricks. |
| R8 | Delayed blast consequences × search re-queries the current shooter's peer/minigame permissions × disconnect, death, leave/reset or permission change during search | The same cut can stop resolving or resolve under a different policy. Carry an explicit admitted damage context and defined revocation/reset behavior. |
| R9 | Bounded partitions × remaining per-blast budget subtracts surviving `made.len()`, not all created/taken pieces × one blast consumes many oversized bricks | Total construction/cue work can exceed the intended cap. Charge every planned/generated cell across the whole admitted blast. |
| R10 | Cheap ordinary shots × `Pieces::new` scans all definitions before discovering there are no eligible big candidates × many stock shots with many Add-Ons | Unnecessary work on the common path. Filter first; cache a validated compatible piece catalog by content revision. |
| R11 | Graceful resource limits × storage/ID admission can return an error from `insert_pieces`, propagated out of weapon stepping × a nearly full world | Convert expected capacity failures to a deterministic, nonfatal outcome with no partial mutation. Preflight and commit belong together. |
| R12 | Predictable weapon scale × current cap fallback removes the oversized original whole × a large load-bearing block at the fragment limit | A budget limit can turn a small bite into a whole-block removal and major collapse. Earlier conversation allowed this fallback, but it conflicts with the stronger long-term predictability goal. Prefer bounded/deferred processing and a defined exact/coarse fallback; do not silently ship the more destructive behavior. |
| R13 | Cheap clump illusion × one compound includes every member's full collision shape × a long or complex detached section | One rigid body is not necessarily cheap. Bound shape/query complexity independently; use simple local motion/contact proxies while the clump is intact. |
| R14 | Locality on the client × spherical `section.radius` expands `Surroundings::reach` in every axis × a tall thin tower | Can load colliders/terrain across a huge mostly empty volume. Use bounded swept extents/probes, not a sphere derived from overall length. |
| R15 | Actual landing breakup × code accepts any static contact while center velocity is downward, excludes kinematic contacts, and requires minimum age/speed × wall grazing, gentle contact, landing on a vehicle/mover | Can break on a side wall or fail to break on landing. Define eligible contact normals/surfaces and sleeping/gentle-contact behavior. |
| R16 | Coherent section lifetime × a missing section key always creates a new body × late batch after landing/expiry/quality eviction | Late members can restart the old fall at the old position. Use an explicit bounded group lifecycle, with completion/retirement and late-arrival policy. |
| R17 | Exact visible geometry × section breakup uses existing whole-debris box spawning × ramps/windows/special shapes in a falling section | Rendered model and collision can disagree. Reuse compatible original collision or a deliberately bounded cosmetic proxy; never imply exact collision from exact volume alone. |
| R18 | Same local-world interactions × pushers, projectiles, later blasts and passage traversal iterate ordinary bodies only × a still-intact section | New clumps bypass parts of the existing cosmetic path. Either integrate each interaction or define and document a deliberate cheap clump policy. |
| R19 | Late join/coalesced updates × original hiding relies on one-shot cues and `dead` retention × joining mid-fight, or hide and event-respawn within one update | Current world state must win; a stale cue must not hide a live original or reveal a fake-dead original through tool ghosts. Give presentation an explicit lifecycle/state basis. |
| R20 | Cheap synchronized events × each detached component hides all members at once and emits bounded but numerous cues × a huge disconnected component or many simultaneous components | Search slicing alone does not bound commit, dirty propagation, mesh rebuild, wire payload, cue pressure or restore. Budget all downstream stages and prioritize essential state over cosmetic detail. |
| R21 | Preserve content identity × `Pieces` takes the first box at a size from the entire catalog × two Add-Ons supply different same-sized boxes | Size alone does not guarantee matching material/attachment policy. Partition palettes need an explicit compatibility/family rule, not arbitrary ID ordering. |
| R22 | Protected foundations × category is parsed from a display breadcrumb, falling back to `base_plate` × absent tool catalog, localized/renamed category, ordinary forced ground block | Authoritative foundation semantics should be explicit source-backed metadata and an authored root role, not a UI-label inference. |
| R23 | Fair cosmetic quality × eviction may drop an entire newly falling section to make room for a few shards × a busy screen at the quality limit | Preserve readable primary motion and degrade secondary detail gracefully, without changing authoritative damage. |
| R24 | Ordinary editable fragments × pieces accept normal tools, while saves deliberately keep originals × painting, wrenching, copying or cutting survivors | The persistence/undo meaning of these edits is not implemented coherently. Set a clear policy at the family transition boundary rather than silently losing edits or duplicating originals. |

Additional validation gaps: `Section` group IDs need semantic validation;
temporary root/family tables need cleanup after all deletion paths; reuse/restart
of IDs or cue epochs across map changes must never revive an old presentation;
and the dormant real-content probe is not yet compiled/accepted and is not
evidence for any of the above.

## Hard-case coverage catalog

Each row needs a stated invariant and an implementation argument before a test.
Later tests should target retained uncertainty, not merely echo the implementation.

| Cases | What must hold |
|---|---|
| Direct rocket into an ordinary small wall; radial neighbor; edge graze; miss | Whole-brick pop remains familiar; each destroyed identity produces material once; misses change nothing. |
| Direct then radial damage from the same projectile; two rockets same tick | No double split, double cue, duplicate event or repeated original reservation. |
| Rocket, grenade, nuke, direct-only and radius-only damage; zero force/volume | Each uses its own authored limits; no hard-coded rocket policy or weak-hit amplification. |
| 64x64 ground block; thin large plate; tall block; odd sizes; all four rotations | Exact non-overlapping stud/plate coverage, correct orientation and stable material, no off-grid slivers. |
| Center bite; corner bite; edge notch; cuts near wall edges; no nearby edge | Cuts remain valid and conserve volume even where snapping to a wall edge is unavailable. |
| Unsupported catalog size; no 1x1; printed box; curved/ramped/window/special brick | An explicit truthful behavior, no guessed replacement mesh or silently missing material. |
| Ground baseplate; upper baseplate; ordinary ground block; floating forced root | Roles stay distinct before/after destruction, loading, duplication and editing. |
| Partly supported foundation on terrain; buried corner; bridge; terrain seam; moving map surface | Ground/support decisions use the actual map semantics and charge their real query work. |
| Second floor supported at four corners, then three/two/one/zero | Any surviving valid anchor path holds the connected section; losing the last one detaches it once. |
| Cantilever; arch; alternating brick courses; hanging roof; ring/loop; narrow bridge between towers | Search follows the complete relevant stud graph in both directions without inventing lateral bonds. |
| Deliberately floating build; unconnected decorative brick; fake-dead support; invisible/non-colliding support | Existing intentional unsupported content stays distinct from newly severed support and temporary fake deaths. |
| One loose brick; several independent loose groups; one multi-floor group | Correct separate grouping and whole-group motion, not arbitrary order-based merging. |
| Skyscraper losing one column versus its final connection | No random collapse from weight or drama; only the actual connectivity change determines what goes. |
| Tower of tens of thousands; million-brick connected build; million unrelated bricks | Discovery, mutation, collision, rendering, cues and restoration all have actual work bounds and eventual progress. |
| Ongoing building elsewhere; new support during search; support removed during search | Local invalidation prevents stale removal and avoids starvation from unrelated activity. |
| Repeated hit on a surviving fragment; fragment itself splits; fragment falls | One original family owns every descendant and timer; removed descendants never respawn independently. |
| Host/guest; owner/foreign owner; trust; public-domain/map-owned bricks; LAN/internet | Same explicit permission policy on direct, radial, split and detached consequences; no privilege escalation by fragmentation. |
| Brick damage off; outside minigame; leave/join/reset/end; shooter disconnects | No unauthorized consequences and no orphaned in-flight structural work. |
| onBlownUp event; immediate respawn/unhide/delete/fakeKill; named targets; delayed relay | Exactly-once authored events and coherent transitions, without resurrecting both original and descendants. |
| Manual respawn; automatic respawn; never-respawn; game reset; map change | Complete family cleanup; correct clocks and original properties; no work from the previous world leaks into the next. |
| Save during split; during fall; during partial restore; autosave; shutdown/reload | Saved logical build is valid and original-only; no runtime physics IDs, transient pieces or stale hidden flags. |
| Hammer/wand; undo; duplicator copy/cut/supercut; paint/fill/wrench; clear-own/group/all | Every authoritative editing path maintains family invariants and has understandable persistence/undo behavior. |
| Player under falling roof; player on it; vehicle below; seat/spawner/portal in section | Authority removes support normally; cosmetic clumps never crush, block shots, carry players or create new permanent rubble. |
| Flat landing; edge landing; side-wall contact; slope; stairs; moving surface; water; no floor | Readable and bounded local landing/breakup/lifetime policy, with no impossible restart or permanent clump. |
| Short drop; long drop; no effective drop; already resting; very slow frame | Clumps still resolve and expire; dropped physics time cannot extend lifetime or generate an unbounded catch-up burst. |
| Quality Off/Low/Best; quality changed mid-fall; low frame rate; low VRAM | Identical authoritative holes/support; graceful presentation degradation, bounded bodies, meshes, uploads and queries. |
| Another explosion/projectile/player/vehicle contacts a clump; portal travel | Consistent documented cosmetic interaction policy, with no feedback into server gameplay. |
| Late join/reconnect; cue drop/duplicate/delay; delta coalescing; partial group arrival | Current existence wins; no duplicate material or stale replay; bounded group/cue bookkeeping. |
| Two same-size brick definitions; missing client asset; Add-On content reload | Stable compatible content identity and truthful fallback; cache invalidation cannot change authoritative meaning. |
| Hundreds of repeated blasts; overlapping respawn cycles; long session | No growing family/root/group/model tables, escalating sound/particle clutter, churn spikes or novelty fatigue. |

## Implementation direction after the review

Keep the product direction. Change the first-pass implementation before adding
more scenario probes:

1. A single original-family lifecycle owns descendants, saved flags, original
   events, timer, generation and remaining material continuity. Internal
   partition boundaries must not silently become extra fractures. All mutations enter that boundary, including
   events, tools and administrative operations. This removes repeated cleanup
   exceptions and makes coalesced updates explainable.
2. A real deterministic work budget charges footprint probes, graph edges,
   partition cells and commit work, retaining cursors and invalidating locally.
   A count of vertices or rigid bodies is not a sufficient performance contract.
3. Intact clumps use cheap local motion and bounded contact proxies/queries;
   ordinary debris physics takes over at breakup. Cap render/collision/unique
   mesh cost independently. Cosmetic quality never changes the hole or support.
4. Original compatibility and current authoritative lifecycle determine
   materials, roots and presentation. Display names and cue arrival order
   cannot substitute for those semantics.

The earlier 3 integrated server checks and client unit pass establish only
their narrow cases. They do not close this review. No playtest-ready claim,
fresh benchmark or full-scenario acceptance is made at this checkpoint.

## Playtest hardening disposition

The inspection above is retained as the before-state, not a current list of 25
unfixed bugs. Maxwell subsequently requested the obvious fixes and a playable
candidate/PR, without turning this into a perfection project.

- R25: same-original surviving faces remain connected in the support proof.
- R1/R2/R9/R10/R11/R12: resumable cell/candidate cursors, local invalidation,
  cached namespace palettes, total allocation admission and nonfatal intact hold
  at unsupported/capacity limits. No whole-wall amplification fallback.
- R3/R4/R5/R6/R7/R8/R19/R22/R24: common Simulation family transitions, preserved
  flags, explicit replicated destruction state, captured admission context/round,
  catalog foundation metadata and deliberate edits committing remaining geometry.
  Box/stack selection excludes hidden originals. Copying survivors copies the
  remaining geometry; it does not copy the hidden original alongside its parts.
- R13/R14/R15/R16/R18/R23: analytic local section motion, shared probe budget,
  upward floor contact, bounded retirement, primary section admission ahead of
  extra shards. Intact clumps deliberately have no player/weapon/portal response;
  ordinary constituent debris retains the existing interactions after breakup.
- R17/R21: deliberate supported limits: ordinary whole debris retains its existing
  approximate collision proxy; partition replacements must be plain catalog boxes
  from the same namespace. No arbitrary other mod's shape substitutes for a size.
- R20: section/candidate commits and original respawns are sliced, collision work
  is batched, and cosmetic members obey Physics Quality. This is a counted work
  bound, not a proven frame-time guarantee for every possible million-brick
  chunk or every renderer/driver. The ordinary integration gate and bounded
  real-content timing are technical evidence; subjective feel and extreme
  skyscraper limits remain playtest feedback, not acceptance claimed in advance.

Current mechanism/policies are in [brick-destruction.md](../architecture/brick-destruction.md).
Final commands/results and package/PR identity belong in the new handoff receipt.

The subsequent pirate-mast follow-up supersedes member-based quality admission
and landed-member retirement: visible art, shared motion and independent body
budgets are separate. Complete membership metadata accepts late arrivals,
shape-aware motion precedes connected landing groups, and eight proved server
groups share removal rounds. See the [preview.2 receipt](../progress/2026-10-27-complete-collapse-assemblies.md)
for evidence, remaining approximations and the interrupted full-gate status.
