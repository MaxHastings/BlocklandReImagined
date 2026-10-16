# v0.3.2 authoritative pose state proposal

Status: documentation for root review; no additional production grant. Immutable
implementation baseline: `63edd2300eaec1a9f8a63ad9073ab6263d45eb96` (D1).
Root accepted D1 as bounded retained native inputs/read-only diagnostics on
2026-10-16. It remains outside v0.3.1. This proposal does not widen its supported
providers or change ordinary hosting, gameplay grips, camera or shot origins.

## Existing owners and dependency edges

`sim/src/session/animation.rs` owns admission, native clip identity, playback
identity, anchor/rate changes, terminal outcomes and bounded records. Its private
`ActiveAnimation::phase_at` uses the existing world tick constant. `animation_view`
returns anchor phases, not phases evaluated at the view tick. `finish` removes
the active entry. Consequently a caller cannot obtain final-key visual hold
from the active view after Natural. Draining `take_animation_records` to rebuild
pose state would steal callback evidence and introduce another state owner.

`sim/src/session/body_pose.rs` owns D1's immutable rig/definition receipts. The
single existing animation-asset transaction validates and installs those inputs.
It does not own mutable actor pose. Content owns pure fitting, socket composition,
layer planning and native channel sampling. It must not import Session or weapons.

`client/src/avatar.rs` still owns locomotion mode/direction/phase, last sample
time, frozen Channels and transition start, stand timing, crouch instance and
layer applicability. It also owns render caches and GPU-derived output; those
remain client responsibilities. `CrouchThread` is already implemented in
`motor/src/crouch.rs` and re-exported by sim/client. There is no missing crouch
algorithm to copy into content.

`bri-motor` already depends on `bri-content` and owns PlayerState/archetype/crouch
mechanisms. A later shared body-motion state module there can use content's pure
Channels/rig helpers without a new dependency or reverse edge. Session can own
one instance per actual actor; Avatar can delegate presentation evolution to the
same mechanism. Weapons already exposes actual `animation_owner`,
`animation_item(slot)` and `animation_image(hand, emote)` identities. Pose state
must consume these identities, never synthesize package counters.

## First independent step: evaluated active phases (D2a)

Proposed production reservation ONLY:

- `crates/sim/src/session/animation.rs`: a read-only evaluated-active view using
  the existing `phase_at` implementation.
- `crates/sim/src/session.rs`: exports only if the new result type needs them.
- A new focused sim integration test, e.g. `tests/evaluated_animation_phase.rs`.

Conceptual API: `Session::evaluated_animation_view()` returns the Session tick,
existing revision and active entries containing their existing playback/request/
binding/clip identity plus an explicitly named evaluated phase. Keep the current
anchor fields and `animation_view` semantics intact. Evaluate at Session's
current tick, with no caller-supplied future tick, cue timestamp, wall clock or
hard-coded tick divisor. This is a snapshot query: advancing ticks changes its
evaluated phases even when the mutation revision stays unchanged. Consumers must
not cache it by revision alone.

Expose clock phase as clock phase. Sampling code applies the native clip's
loop/clamp rules; it must not replace native key extent with a source callback
deadline. The normal finite entry disappears through the existing advancement
path; D2a intentionally cannot supply a visual hold after removal. No channel
mapping, pose cache, producer, outbox, completion or wire change belongs here.

Acceptance: actual Session admission and stepping establish rate-1 and changed-
rate evaluated phases; query twice has no side effects; original anchor view is
unchanged; duplicate start preserves identity/anchor; replacement and lifecycle
invalidation remove the old entry; looping, zero-duration and Natural removal
retain current behavior. Assert the outbox is unchanged by queries and existing
15 authoritative-completion tests remain green. Characterize exact pre-change
clock behavior before edits. Run changed-crate quick and scoped focused with
process-local `CARGO_BUILD_JOBS=2`, leaving `CARGO_TARGET_DIR` unset.

Root can grant this step independently of all following design choices.

## Separate decision: visual final-key retention at the clock owner (D2b)

This requires a separate grant and applicability/budget review. A retained pose
is not an active playback and is not a terminal callback. Proposed read-only
pose entries distinguish `Playing` from `HeldFinal`, retaining the same actual
PlaybackId, target, qualified binding and native resource/digest/sequence identity.
The clock owner writes them atomically with its existing operations. It must not
start a new timer, replay terminal records or keep an active clock alive merely
to preserve channels.

For an explicitly eligible target, Natural emits its one existing terminal record
and retains the native final sampling position. A repeated query cannot emit
Natural again. Looping entries remain Playing. Same-tick replacement changes the
pose atomically with admission and retains the existing Interrupted outcome.
Duplicate admission cannot resurrect a cleared hold. A finite zero-duration
entry still produces Started/Natural atomically; any eligible held representation
must be installed in that same transaction. Rate commands after Natural retain
their current stale-playback behavior; they cannot restart a held pose.

Stop/root after Natural can clear a visual hold without manufacturing another
terminal outcome. Current native `root` cancels active playbacks only, so the
visual clear needs an explicit path at this same owner. Existing owner life/body,
asset catalog and applicable custody/mount changes must clear invalid held state.
Interrupted/Cancelled holds, if any source requires them, need an explicit reviewed
policy; do not infer them from Natural or from a generic stop.

Retention eligibility cannot be guessed for every admitted channel. Current
Pawn admission accepts channels below 32; legacy effects/client body consumers
use their existing narrower slots. This proves neither Pawn15 layering nor an
image-to-body mapping. Specify the supported target and producer policy before
implementation. In particular legacy body thread2 currently associates action
applicability with the whole sorted mounted-image set, while a free native body
producer can carry only an owner binding. That binding alone cannot preserve
the client's equipment-change invalidation semantics. Capture/query the actual
existing mount generations at the authoritative producer; audit all hand/emote
enumeration and replacement paths before reserving those production paths.
Image animation remains its own target.

Boundedness must be designed before admission changes. Existing limits include
128 active playbacks, 512 records, reserved terminal capacity, 256 receipts and
4096 nonces. A hold must not consume a terminal reservation forever. Define a
bounded retained-target set from supported actors/targets and preflight any
capacity requirement alongside existing admission, before mutation. Do not
silently truncate held targets, evict a valid pose or leave half a catalog swap.

Acceptance adds real Session tests for Natural followed by persistent final-key
sampling, one terminal only, clear-after-Natural, same-tick replacement/restart,
duplicate after clear, zero duration, loops, rate boundaries, actor death/respawn,
body/catalog replacement and qualified item/image custody changes. Failed
admission/catalog replacement must preserve active and held state together.
Whole-mount applicability needs same-ID redraw and other-hand changes, not only
different item names. These tests establish state semantics, not source callback
or original asset fidelity.

## Remaining shared mutable body-motion inputs

Proposed later owner: `crates/motor/src/body_animation.rs`, exported by motor's
`lib.rs`; Avatar delegates in `crates/client/src/avatar.rs`. This is a proposed
reservation, not permission to edit those paths. First extract the existing
client mechanism with native before/after controls; only then bind authoritative
Session inputs in a separate step. No manifests or lockfile changes are needed.

| Input/state | Existing owner and behavior to preserve | Later authoritative requirement |
| --- | --- | --- |
| Mode, direction and phase | Avatar changes action only when sequence string changes; same sequence with another direction does not restart. Resolved roles, reversed movement fallback and selected owned seat clip affect sampling. | Supply actual body definition, grounded/velocity/jet/posture state, water coverage, intent/death and resolved seat eligibility. No content-name mechanics. |
| Time and rate | Avatar's first elapsed is zero; subsequent elapsed clamps to 0.25 s. BodyKind uses native ground motion, local velocity/body scale and existing rate limits. | Caller supplies authoritative tick elapsed through existing Session time. Client retains its presentation time source; no global server-tick replacement. |
| Frozen transition | Previous Channels and transition start; normal blend 0.25 s, jump 0.15 s. Separate acted/unacted boundaries already exist in the shared sampler. | Retain actual previous sampled channels across transitions; do not interpolate from bind pose or re-create history from cues. |
| Crouch | Existing motor CrouchThread position/scale/active/start; initial crouch begins at end, recrouch during rise resets, stand reverses and root parks at zero. | Reuse the type and law. Skeletal eye composition remains separate from the existing scalar crouch eye keys. |
| Seat and support applicability | Exact owned seat clip, resolved coverage and whole-image eligibility; unacted channels are requested only when acted and wanted. Support can resume at duration while action channels retain their final pose. | Supply actual mounted-image state and resolved contracts; completion/held status alone cannot select support policy. |
| Stand timing and history | Avatar's stood_at, jet-flight correction, dead state and current body policy participate in selection/composition. | Explicit observed inputs/history; no guessed source sequence or hidden second clock. |
| Lifecycle | Same-player outfit rebuild uses continue_animation to preserve motion/crouch/transition history. New spawn clears it. Full body-kind equality invalidates resolved assets/fits; composition/stand-in changes reset crouch. | Keep identity-preserving rebuild distinct from actual body/life replacement; reuse existing owner generations. |

The first extraction should preserve present behavior, including awkward same-
sequence alias/direction cases. New role policy is a separate change. Look mapping
(`Avatar::look_position`) remains client-owned pending an explicit pure-mechanism
grant. GPU buffers, meshes and posed-node caches do not enter motor or Session.
Authored prone must not receive an additional inferred rigid prone rotation.

Before host pose composition, enumerate every adapter input as actual Session
state, explicit unsupported state or a separately owned provider. D1 does not
support stand-ins or separately vehicle-owned/load_horse rigs. A vehicle consumer
needs its actual effective frame/scale, pre-passenger-turn fallback point and
mount sample from the existing vehicle owner, with separately reserved paths.
Never substitute a stock body or manufacture a seat transform to close that gap.

## Source evidence and independent acceptance

Private evidence is read-only in the Add-Ons repository; no original content is
committed here. These are pinned requirements, not claims that D1 executes them.

- `fbdd03ead1c943229a34de68b0a2e5463ef7c094`,
  `aa25/notes/2026-10-16-native-reload-timing.md` and
  `2026-10-16-reload-clock-consumer.md`: M16 native visual span is 140/30 for
  141 keys at 30 fps, while dedicated GetAnimDuration is 141/30 at rate 1 and
  tween 0. Standalone/listen native clamp and dedicated polling/AnimEnd are
  distinct source paths. Do not stretch native sampling to that deadline or
  guess a timer from Sleep polling. FinishAnim can release on an early stop
  without Natural AnimEnd. Pawn15 notification cannot finish the weapon;
  independent tween/fades are not extra native clip duration. Stage1 is still
  tween-zero only; endpoints and package completion policy remain open.
- AA actor PR47 `49104a6`, `aa25/notes/2026-10-16-source-actor-native.md`:
  18 controlled native body/rifle/CameraBone matches coexist with ordinary-prone
  double rotation (up to 1.698112) and scalar-eye versus CameraBone mismatch
  (up to 0.491854). Shared phases and static fits alone do not close camera/fire.
- AA PR54 `def522999e9e9413a9a2c947f5504a8eeb08995b`,
  `aa25/notes/m16-audio/activation-v032-handoff.json`: old-pin package-view audit,
  not an executed Stage1 counterexample. Re-evaluate the existing owner/item/
  image generations on the exact consumer SHA before proposing another API.
- `fbdd03ead1c943229a34de68b0a2e5463ef7c094`, Halo
  `WARTHOG-PASSENGER-COMPOSITION-HANDOFF.md` and
  `WARTHOG-PASSENGER-FALLBACK-5E728-HANDOFF.md`: authored seat/ready and sparse
  AR reload channels must preserve seated pelvis. Two-axis nine-frame aim,
  acceleration overlays and source fire-event laws remain independent open
  inputs. Neither a one-dimensional aim guess nor visible socket motion defines
  gameplay muzzle policy. Tire/contact timing and hit-basis work are separate.

Mutable-state extraction acceptance must compare existing Avatar native output
before/after for grounded directions/ties, reversed clips, missing roles, jump/
jet/water, crouch/reverse/recrouch, transition interruption, same-sequence aliases,
seat ready/support applicability, held action, outfit continuation and new spawn.
Use real native sampling with invented fixtures and existing actual Avatar tests;
tests that simply echo the new state fields are insufficient. Preserve both acted
and unacted output and existing WorldItems attachment controls.

Host acceptance later needs an actual Session test, not just a pure composer:
real boarding/switching and scaled/tilted seat frames, action and binding lifecycle,
actual wall fallback and phase at the weapon firing boundary. Existing order is
`sync_seated -> advance_animations -> step_weapons`; a granted host sample belongs
after the first two and before firing. A cue produced after firing cannot move
that shot retroactively. Reserve grips, weapons adapter and vehicle accessor only
after root reviews this input/state chain. Original Halo/AA consumer checks run
only when root routes their exact immutable SHA. Maxwell performs interactive
playtests. No callback, wire, Pawn15 mapping or original-fidelity acceptance is
closed by this proposal.
