# Independent contact vectors and authored effect groups

Design/strict-regression checkpoint on `codex/contact-effect-frames`, based on
camera correction `3cf62a6b0229b9df9f3c5b299f8c1fb4a2988a30`. The original
reservation covered only the new test, this note, own progress and one seam row;
root subsequently granted the implementation described below.

Root subsequently granted the bounded implementation on
`codex/contact-effect-frames-impl`. The proposal below records its source evidence;
current implementation and commands are recorded in
`../progress/2026-10-16-contact-effect-frames-impl.md`. All strict tests are enabled
in the corrected branch. A separate immutable diagnostic branch retains the
original failing requirements.

The implementation uses ContactGeometry, Source-default BurstFrame and opaque
native CompositePlan preflight. Malformed provided geometry/pose/group data fails
before any batch cursor or emission mutation. Missing contact skips/counts only
that group; capacity admission reserves eligible groups' existing conservative
source/light/count-max budgets and emits nothing on rejection. Unsupported
contact attachment is a separate diagnostic/omission, not shooter attachment.

Private Halo handoff: PR 34, immutable
`fa09e005e2e7e3ea315f09c5c0c6f2697910c179`,
`halo/AR-CONTACT-FRAMES-HANDOFF.md`. Its real ordinary Session wall hit has
non-collinear travel/normal, but the effect cue carries only the normal. The
original PC effect groups independently name reflected, incident and opposite
incident locations; water additionally names a gravity location. This proves
independent frame selection is needed. It does not prove the PC runtime's
remaining basis, signs, roll or object/node attachment law. Other-version
reconstructed code is corroborating evidence, not PC acceptance.

## Existing mechanisms and the first loss

`weapons/runtime.rs::hitscan` passes the actual spread/muzzle-corrected ray to
`hitscan_land`. The latter constructs a `ProjectileContact` with its actual
velocity, collision normal, position, target and shooter attribution. It emits
`Event::Contact` and invokes ordinary `Query::on_contact` before effects. This is
ground truth; nominal camera aim and a client-drawn tracer are not replacements.

`WeaponsWorld::effect` emits `Event::Effect` with one optional direction.
`explode` supplies the normal at a landed contact. The special hitscan explosion
projectile can overwrite velocity with that normal, so reading its velocity
later is already too late. Flying projectiles and bounce/stick branches need
the same audit: capture pre-response incoming velocity, before reflection,
redirect, zero/rest or death changes it. Lifetime expiry has no contact and must
not acquire fabricated vectors. Surface effect selection remains unchanged.

`sim/session/weapons.rs` forwards the one direction to
`presentation::CueKind::WeaponEffect`; reliable transport preserves that same
shape. `client/weapon_effects.rs::cues` derives one SourceTransform from the
direction by the existing +Y minimal-arc rotation. The effect is not attached
merely because its attribution is an actor; image/node/duration drive existing
attachment selection.

Reuse these native mechanisms:

- `content/effects.rs::Burst`: bounded per-group emitter/count/radius, local
  offset/direction and follow_source. A composite already carries up to 32
  independently authored Burst groups. Do not add a parallel explosion system.
- `fx-runtime/world.rs::play_composite`: lowers each group's local offset and
  direction into a source transform, then uses existing burst emission, seeded
  count/radius choices, budgets and particle lifetime. Add reference selection
  immediately before this existing lowering.
- `ParticleOrientation::{Screen, Axis, Plane}`: screen/ribbon/fixed-plane
  presentation is independent of the selected emission reference. Plane already
  retains a right vector and spin. Do not replace it with a camera-facing plane
  to hide a missing contact basis.
- Existing `SourceOptions::follow_source` / `carry_attachment` keep particles
  in their declared attachment frame. They do not identify the contacted object.

The existing cosmetic particle `ContactImpact` already separates incoming
velocity and normal for swept particle audio. That is a different producer and
must not be used as a substitute for the authoritative shot's accepted contact.
Original audio repeat/promotion, cut-plane appearance and wood collision/render
separation remain separate unclosed requirements.

## Reviewed transport contract

The implementation adds a bounded plain geometry record,
`content::effects::ContactGeometry { travel: [f32; 3], normal: [f32; 3] }`, and
optional `contact` on native `Event::Effect` and `CueKind::WeaponEffect`.
Existing effect position remains the contact point; do not duplicate it in the
record. Both vectors are finite, nonzero unit vectors from the same accepted
hit. Validate length with the existing cue tolerance; reject invalid provided
records. `None` retains existing semantics. Do not encode vectors into source,
node strings, scale, image identity or the legacy direction field.

Keep `direction` unchanged: it has existing aim/normal/lifetime meanings and
unrelated effects depend on them. A new explicit contact record removes the
need to infer which meaning a direction had. Shooter attribution remains
`source`, even when the contacted target is a different actor or vehicle.

Producer helpers take optional contact explicitly; collision callers capture
it before changing projectile velocity. Pass it through material selection,
Event, Session and Cue validation without reconstructing it. Tests must cover
ordinary hitscan, nonzero randomized spread, the explosion-projectile override,
flying impact, bounce/stick pre-response direction and non-contact expiry.
Do not infer attachment from this geometry-only record.

The reliable cue change is recorded by exactly one new file,
`crates/net/protocol-changes/contact-effect-frames.md`, describing the optional
contact record on checkpoint/delta presentation cues. Never edit VERSION
directly. The diagnostic did not create it; the granted implementation does.
Content remains under the existing explicit alpha Library/weapon schemas as
a defaulted extension; no migration is introduced. Default omission preserves
the existing authored behavior.

## Reviewed per-group reference contract

Existing Burst gains a defaulted typed reference selector,
`frame: Source | ContactNormal | ContactTravel | OppositeTravel |
ReflectedTravel | WorldDown`. These are explicit engine geometry operations,
not Halo tag-name branches or source-policy bindings:

| Reference | Native reference axis |
|---|---|
| Source (default) | Existing source/socket +Y, including legacy contact direction |
| ContactNormal | Contact normal n |
| ContactTravel | Actual incoming travel d |
| OppositeTravel | -d |
| ReflectedTravel | d - 2(d dot n)n |
| WorldDown | Native world -Y |

Burst.direction remains a local ejection direction relative to the selected
reference, and offset belongs to that same frame. Keep current Source lowering
bit-for-bit. WorldDown uses the native world axis without needing contact data.
Explicit contact references without actual contact data are
unsupported: count/report the missing context and omit that group, rather than
inventing an incoming vector. Sibling legacy groups can still emit within the
existing budgets. Root chose whole-batch validation of malformed provided
geometry/pose and preflight of every group before cursor/emission mutation.
Missing optional context omits/counts only that group. Capacity admission uses
only eligible groups and the existing conservative count-max semantics; rejection
emits nothing and consumes the reliable cue once. No retry or later replay.
Capacity rejection is per composite: earlier successful cues in the batch remain
accepted. In contrast, malformed provided geometry/pose/group data rejects the
whole batch before any of its cursor, host-request or emission changes.

One forward vector does not specify roll. Reusing current minimal-arc machinery
is a documented native convention for a bounded initial reference selector;
it is not proof of an original marker matrix. A verified full original basis
must later be authorable without loss (a validated local quaternion/basis
override is a possible extension to Burst, not a new particle framework).
The bounded mechanism does not claim to express the original full plane basis.
The source owner still needs PC evidence for sign, perpendicular/roll and local
coordinate conversion. No binding is authorized merely because a reference's
name resembles an original marker. Offset/local direction and plane orientation
must compose through the same selected basis exactly once.

`follow_source` does not solve contact attachment. Contacted target/node is a
separate demonstrated gap and must be transported explicitly if attachment is
required. With only geometry, contact effects are fixed world contacts; a group
requesting unavailable target attachment is explicitly unsupported. Never
attach it to the shooter or overload the cue's source/node/scale. A later typed
attachment record needs stable target identity, actual node/frame semantics,
death/removal and late-client behavior under its own reservation.

## Approved production footprint

| Paths | Bounded responsibility |
|---|---|
| `content/src/effects.rs` | Plain contact geometry, typed/default Burst reference and validation |
| `weapons/src/runtime.rs`, `weapons/src/lib.rs` | Capture accepted incoming/normal before mutation, optional Event field, preserve typed effect validation and defaults |
| `sim/src/presentation.rs`, `sim/src/session/weapons.rs` | Optional cue field, validation and exact forwarding |
| Existing native cue constructor sites in sim `session.rs`, `session/{packages,tools,spray,breakables}.rs` and corresponding tests | Explicit None for unrelated effects; no semantic changes |
| `fx-runtime/src/world.rs`, `fx-runtime/src/pack.rs` | Per-Burst reference at existing lowering; context validation/budgets; legacy Source unchanged |
| `client/src/weapon_effects.rs` | Deliver explicit contact context without conflating attribution and attachment; preserve cursor, fallback and source transform behavior |
| `net/protocol-changes/contact-effect-frames.md`, affected net codec tests | One wire-change marker plus actual reliable roundtrip/rejection checks |

No runtime Torque reader, VM, new effect framework, game-name branch or camera
aim reconstruction. No manifests/dependencies are required. Root owns additional
path reservations, shared integration, gate and release. Constructor-only and
corresponding test additions are listed in the implementation progress entry.

## Public evidence boundary and next acceptance

The new public test uses invented WeaponsWorld/Query geometry with real native
spread and accepted contact callbacks. It compares contact velocity against
the actual queried cast, proves that nominal aim differs, and keeps shooter
and map target distinct. At diagnostic checkpoint `27678b2f`, strict ignored
requirements exposed missing Event
context, loss of proposed context through actual native Cue serde and unsupported
authored Burst reference selection. The third case has an actual WeaponEffects
consumer and independent normal/travel particle-output assertions ready after
schema support; that checkpoint fails at Burst's unknown-field rejection. They are
not a fabricated Session adapter or proof of reliable binary serialization.
The private PR 34 independently records an actual ordinary Session failure.

The green native FX control exercises existing local Burst direction and Plane
orientation under a rotated source. It does not bind any original tag or assert
an unverified reflected/incident sign, roll or attachment law. The reviewed
field names now implement retention of the actual independent quantities. The
corrected target enables all of these cases and adds actual Session, projectile
response and budget/cursor coverage; net replication exercises the real reliable
codec and atomic Replica rejection. Exact results are in the implementation entry.

Implementation acceptance requires the enabled strict cases plus actual
Session/reliable transport and per-group non-collinear native output checks,
default-preserving legacy and absent/invalid context controls, contact expiry/
bounce/override paths, attribution/target separation and budget/cursor behavior.
Then pin an immutable SHA for Halo's original consumer. Cut-plane, collision,
full basis/roll and target attachment keep explicit owners/evidence requirements;
matching two axes alone cannot close original fidelity.
