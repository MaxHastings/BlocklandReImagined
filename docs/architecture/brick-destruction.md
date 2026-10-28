# Brick destruction playtest design

Maxwell authorized completing Claude's design on his PC on 2026-10-27.
This explicitly expands the older structural-collapse exclusion for this
bounded brick-destruction feature. The broader alpha contract stays open.

## Experience

Keep familiar whole-brick pops, believable blast scale and readable debris.
Fun takes precedence over physical fidelity: make uncommon plausible outcomes
accessible without increasing every explosion. No artificial/generated sounds,
unrelated shake, oversized particles or added visual material. Existing game
sounds may receive restrained pitch, level and timing variation; no new audio
is required to deliver this increment. Cosmetic acceptance belongs to Maxwell.

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
Physics Quality can cull cosmetic debris as it already does.

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

Server work resumes across ticks: footprint rays, spatial candidates and stud
attachment cells share 8192 work units; blast and support phases share the budget.
At most 128 candidate changes/section removals occur per tick. Split allocation
has 2048 cells per admitted blast and across the tick, and 16384 active surviving parts per world.
There are at most 256 pending blasts; overflow is diagnosed and rejected.
Respawns drain 128 originals per tick, with collisions refreshed in a batch.
Nearby topology changes invalidate pending proofs; unrelated far builds do not.
Permissions/round attribution are captured on admission; reset cancels stale work.

Client presentation separates visible material, assembly motion, and physics.
Every admitted original member keeps its mesh, print, color and shape. A section
shares one transform before landing. Its complete membership count, bounds and
lowest footprint nearest the hit are fixed one-shot cue metadata, not a pose
stream. Tall sections lean around that footprint before falling free; wide
slabs drop and tilt; long narrow spans rotate. Eight cached actual member
corners sweep to the next pose under a shared 256-candidate contact budget.
Only upward floor contacts trigger landing; this remains cosmetic contact,
not a structural solver or exact irregular-body collision.

Late batches join the current falling or landed pose. Breakup waits for the
reported total, so a landed first batch cannot discard later members. If a
section is interrupted by world edits or cue delivery is incomplete, the
received members retain their shared pose and fade normally rather than
inventing missing art or restarting. Expired groups retain metadata for 30s.
The normal 13-second visual lifetime includes the fall.

After complete delivery, connected face-neighbor region growth plans compact
assemblies with a shared 4096-candidate budget per frame, starting during the
fall. Physics Quality controls target granularity (roughly sqrt(member count)
at Best, scaled down at lower settings). All members stay visible, each with
one assembly transform; original per-look mesh/texture instancing is reused.
At most 64 assembly bodies share the existing global rigid-body budget with
ordinary blast debris. Each assembly has one box proxy; assembly proxies
ignore one another to prevent empty bounding-box overlap from exploding the
structure. They collide with the map, surviving bricks and ordinary local
pushers/debris. Sparse group contact is consequently approximate.

Assembly collision streaming uses its swept oriented bounds, not a tower-sized
sphere, and shares 512 work units per frame (16 per terrain tile). Motion waits
while nearby collision is prepared. Sleeping/budget-evicted assemblies retain
their last pose and all their visible members through the normal fade. Physics
Off keeps the cheap intact fall/landing visual without independent bodies.
There is a separate 65536-member render reservation and 1024-group metadata
ceiling; excessive new groups are diagnosed and omitted as whole groups rather
than arbitrarily truncating their members. This is not unlimited skyscraper
rendering. Intact falling sections cannot crush, carry or block gameplay;
post-landing assemblies remain entirely local, with no motion replication.

Up to eight proved-loose server groups drain removals round-robin within the
same existing 128-change tick budget. Proof discovery is still incremental;
this removes delivery serialization, not the cost of proving connectivity.
Nearby direct damage and authored edits both invalidate queued proofs.

Compatible split sizes come from the same content namespace, cached only when
an oversized plain brick needs them. Unsupported shapes/prints/catalog sizes
retain the original weapon size rules. This does not invent mod geometry.
Non-box whole debris retains the existing approximate collision proxy; improving
that pre-existing cosmetic approximation is outside this playtest increment.
