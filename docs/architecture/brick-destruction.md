# Brick destruction playtest design

Maxwell authorized completing Claude's design on his PC on 2026-10-27.
This explicitly expands the older structural-collapse exclusion for this
bounded brick-destruction feature. The broader alpha contract stays open.

## Experience

Keep familiar whole-brick pops, believable blast scale and readable debris.
Fun takes precedence over physical fidelity: make uncommon plausible outcomes
accessible without increasing every explosion. No artificial/generated sounds,
unrelated shake, oversized particles or added visual material. Existing game
sounds retain their identity. Maxwell also authorized verified licensed real
recordings for restrained plastic release/contact/landing cues on2026-10-28;
see [recorded audio](../modding/brick-destruction-audio.md). Cosmetic acceptance
belongs to Maxwell.

Latest instruction supersedes the earlier optional switches: the full
experience is enabled in free build and minigames. No new destruction/shards
toggle. Existing damage permissions and Physics Quality continue to apply.

- Small eligible bricks pop whole under the weapon's existing size rules.
- Oversized plain box bricks partition on studs/plates into exact catalog
  bricks, fine near the blow and coarse away. Neighbor edges guide floor
  cuts where practical. Only reached pieces within the weapon limit fly;
  surviving pieces are ordinary authoritative bricks and can be hit again.
- Ground-supported one-plate slabs in the Baseplates category are protected foundations;
  tall Cubes share that category but remain ordinary breakable blocks.
  Upper-floor/roof baseplates are ordinary destructible slabs. Ordinary
  large ground blocks can split. Existing force-planted floating roots stay
  anchored; a roof must not become a root when its last support disappears.
- A blast can detach a section only by severing its live stud connections.
  The server hides its bricks for respawn and groups their existing death
  cues. Each client alone tumbles the section and separates its constituent
  bricks on landing. No crushing damage or moving debris in server physics.
- Direct-hit plain bricks occasionally crack into complementary jagged
  shards. A brick's shards exactly reassemble into that brick; there is no
  filler material. Most hits remain whole. The initial internal chance is
  25% of eligible direct-hit cues, deterministic per cue; this is a game
  tuning choice, not a claimed ABS fracture probability.
- Original identities own events, respawn and saves. Mid-fight saves contain
  originals, not transient partitions. Respawn removes every descendant and
  restores the original once. Existing player-overlap respawn behavior applies.

## Cost and limits

No continuous motion replication. Authoritative brick additions/removals and
one-shot death cues still cost bandwidth proportional to affected bricks;
this is not a constant-size message per explosion. A group ID rides those
cues, with no replicated rigid body, pose stream or damage attribution.

Split allocation, active transient pieces, support-search work per tick and
client debris bodies are bounded. Search starts at affected connections, steps
toward lower supports first, and retains unfinished frontiers across ticks.
An unsupported component may require exploring that component; it cannot be
proved disconnected using only a fixed-radius local query. Unrelated builds
are not flooded. A changed support topology invalidates pending conclusions.
At the partition/brick budget, retain the oversized original unchanged; a
capacity limit must never turn a small rocket into removal of an entire wall.
Physics Quality controls independent motion. Ordinary blast debris retains its existing cosmetic budget; section admission reserves visible material before hiding it.

The cosmetic debris world uses two constraint-solver iterations per120Hz step.
This reduces dense-pile/catch-up cost by trading precision in visual stacks;
CCD, collision geometry, motion frequency and material budgets are retained.
Authoritative physics has its own parameters. See the
[measured correction](../progress/2026-10-28-cosmetic-debris-solver-cost.md).

Initial implementation supports catalog-representable plain boxes. Curved,
sloped, special and assigned-print bricks retain their existing whole-brick
size rules; no guessed collision box or print mapping replaces their shape.
These limits must appear in the playtest handoff and are not fidelity claims.

## Acceptance before handoff

Integrated volume and repeated-hit checks; upper slabs, roofs and foundations;
floating roots; permission denial and disabled minigame damage; original event
inputs, save/restore/reset; local landing breakup and bounded lifetime;
network/late-join state; actual-content scale/captures, repeated-blast work and
Windows package checks. Record measured numbers and failures in the progress
entry, distinguish technical/visual checks from human acceptance, and keep the
candidate incomplete until the package and required verification exist.

## Cohesive hardening

`Simulation` owns transient damage families at the common mutation boundary.
Restoring/removing an original retires its parts. Deliberately editing/removing
one surviving part commits the visible remaining geometry as authored bricks
and cancels that original's restoration; it must not overwrite a builder's edit.
Original appearance/collision flags are saved and restored exactly. The explicit
replicated `damage_hidden` flag separates destruction from an authored invisible
or noncolliding brick, including for late joiners. Cue-before-state adapters work;
when a timestamped newer world restores a brick, stale cues cannot hide it again.

Internal partition faces of one original retain material continuity. Ordinary
stud attachment rules still govern connections to other bricks. A live roof
which had supports never becomes a deliberate floating foundation just because
its final support was removed. Category comes from catalog metadata.

## Shared section lifecycle

Simulation owns both original damage families and a section journal. A proved
component has stable planned membership shared with its incremental producer.
Only that producer can hide its members without interrupting the operation.
Common edits, restores, deletes and other blasts invalidate pending membership.
The producer seals the actual committed prefix, then reproves the untouched
remainder against the current world. It never promises an unreachable old total.
Originals shared by several fragment sections wait for the latest section's
restoration deadline. No original restores while one of its sections collects.

This is explicitly a streamed authoritative operation: at most 256 candidates
are changed per tick, and uncommitted members still exist for gameplay. It is
not an instantaneous atomic removal of an entire skyscraper. Brick state is
truth for collision, queries and building tools; the common mutation boundary
protects family ownership and pending section proofs. Server gameplay does not
wait for client rendering.

Each operation publishes a Ready barrier with its actual committed count, then
Retired when its remaining source references disappear. Source bricks carry a
transient section tag, including packed replication. Clients stage received
material at its original pose until Ready, missing-source reconciliation,
and GPU model readiness all complete. Contact witnesses prepare during
member admission; the landing index can prepare in flight and does not gate
the first coherent movement. Tagged world
state recovers missing cues without inventing geometry. Restoration suppresses
stale members immediately; retirement prevents late resurrection. The normal
13-second visual lifetime starts after preparation, not during delivery.

## Pipeline admission and budgets

Server traversal, footprint rays and stud attachment cells share 8192 counted
work units/tick. Each support-query credit batches at most16 cheap coarse-bucket
rejects; actual ground rays and authored attachment work retain their quota.
No query eagerly floods the component. At most256 damage/removal candidates and
2048 split cells are
admitted per tick; splits also have a 2048-cell per-blast limit. There are at
most 16384 active surviving parts and 2048 descendants per original. Mutation
payload estimates share 4 MiB/tick. These are work/allocation bounds, not an
absolute wall-time or encoded-network-byte guarantee.

Pending blasts/support jobs are capped at 256, proof/seed retention at 131072,
one component at 65536, and simultaneous removal producers at eight. Searches
share work round-robin; edits invalidate a coalesced conservative watched box.
Support queries that exhaust their neighbor envelope never conclude unsupported
from a truncated proof. Restoration scans 512 timers/tick and admits at most
128 originals, 2049 descendant-weighted mutations and 4 MiB of original payload
estimates per tick. Repeated restore calls in a tick share that credit.

Before hiding a component, the server reserves its presentation envelope for
30 seconds after sealing (or until source retirement): 65536 source members,
256 appearances, 65536 unique mesh quads, 524288 instanced quads, 262144 spatial
buckets and 64 operation leases. A member may occupy at most 256 buckets, one
appearance at most 4096 quads, and one source at most 4 MiB stored payload.
Temporarily occupied capacity keeps a proof waiting. A component intrinsically
outside this envelope stays standing until further damage/build changes cause
another proof. It is not removed and then silently omitted by the renderer.
This conservative overload policy can leave an extreme unsupported structure
standing; arbitrary million-brick collapse rendering is not promised.

Member/control cues take priority over ordinary effects in the bounded 4096-cue
queue. Local presentation backlog drops redundant material cues while retaining
a seed/control per section; tagged authoritative state supplies missing art.
It does not disconnect an otherwise valid client merely for cosmetic backlog.
Brick deltas still cost bandwidth proportional to affected material.

## Client motion, breakup and hits

Visible material, shared transforms and independently simulated bodies are
separate resources. Every admitted member keeps its original mesh, color,
print and shape. The landing index prepares256 members and models upload at
most four new appearances per frame. All received material remains visible.
Complete delivery and GPU readiness permit the shared fall; landing-index work
continues during flight. Tall
sections lean around a lowest footprint near the severed support; wide slabs
drop and tilt; long spans rotate. Corners and 64 distributed material witnesses
probe a 0.2-second contact horizon under a 4096-unit budget. The displayed clock
advances through checked time rather than accumulating a delayed jump.
Contact remains a cosmetic approximation, not a structural collision solver.

Face-neighbor planning shares 4096 work units/frame and incrementally finalizes
landing groups of at most 2048 members. Available body slots and Physics Quality
choose coarser groups. At most 64 bodies share the ordinary debris body budget.
Compound proxies merge only exact rectangular face neighbors, preserving gaps;
they allow 64 boxes/body and 2048 boxes globally. Non-box shapes keep the existing
bounding-proxy approximation. If disconnected groups or proxy/body needs exceed
the envelope before allocation, the section keeps one visible shared assembly.
Already moving bodies are never reassembled; budget eviction freezes their
current poses without losing members. Physics Off retains the shared visual.

Ordinary debris and section chunks use one collision-window mechanism: prepare
swept oriented surroundings ahead while retaining the previous safe window.
Nearby edit history invalidates affected windows; distant edits do not pause
unrelated rubble. Static cache capacity is bounded. Ordinary preparation is
shared through bounded8-unit tiles, including in-progress queries. Completed
coverage is invalidated over its whole area, so a newly built wall cannot be
missed by a moving body. Ordinary preparation retains128 real geometry queries
per fixed step and section preparation512 per frame. Cache lookups cost one
sixteenth of a query; fresh terrain costs16 queries. Both geometry and lookup
work remain bounded. The swept lookahead covers two worst-case frames rather
than a half-second angular sphere. Genuine nearby resource pressure can still
delay cosmetic motion.

Shots use the displayed projectile path. Allocated bodies receive a mass-scaled
local impulse and wake; preparing, frozen and shared assemblies also have bounded
source-box hit queries and restrained recoil. Gaps remain queryable gaps rather
than one giant sail bounding box. This is a local visual response: authoritative
projectiles continue through rubble and trigger no extra gameplay explosion.
First observed segments cover one fixed step; terminal-only unseen trajectories
are not guaranteed. A64-entry recent explosion context preserves observed
surface direction across delayed member packets without replaying its impulse.
Context expires after the ordinary solid-debris interval and resets per session.
Audio uses recorded assets and real local events through the existing mixer;
no synthetic/generated sound or extra gameplay explosion is introduced.

Existing source permission policy is shared by direct and follow-on damage,
including the pre-existing LAN exception. WAN ownership/minigame restrictions
remain authoritative. No additional host or shard toggle was introduced.

The extra transient section pointer is offset by boxing the cold rule-region
array. Serialized rule properties and commands keep their existing array shape;
the per-Brick 256-byte memory ceiling is unchanged. World state owns semantics,
while network, tools and presentation derive views of it.

Technical validation and remaining limits are recorded in the current progress
entry and playtest handoff. Maxwell alone performs interactive acceptance.
