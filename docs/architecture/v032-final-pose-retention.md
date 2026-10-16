# v0.3.2 final-pose applicability and capacity proposal (D2b)

Status: root granted D2b-1 only from immutable proposal
`3d44bfb7264178b17069d695cb4f059996d3b919`, based on accepted D2a
`84dee79add6fba5ba311dab17f7fbf0a0567a624`. D2b-2 remains ungranted.
Keep this lane outside main/v0.3.1. The retained native assets of D1 and the
evaluated active phases of D2a do not supply final holds, pose layering or shot
origins. This document refines, rather than replaces, the remaining work in
`v032-authoritative-pose-state.md`.

## Recommended first code reservation: trusted-host opt-in only

Propose D2b-1 as an independently reviewable mechanism. Support exactly
`AnimationTarget::Pawn(2)` on an existing live Session owner with an admitted
native body clip, a successful D1 body-input receipt for its actual archetype,
and actual existing AnimationBinding. Missing bindings, stand-ins and separately
vehicle-owned/load_horse providers stay unsupported. Retention requires a
separate trusted-host option, conceptually `HoldNativeFinal`, on a new prepared-
start API. The host derives applicability from runtime state; callers cannot
submit a mount-set assertion. Existing `prepare_animation_start` and every
existing producer remain `NoHold`. No global setting or automatic promotion of
legacy requests. This establishes a host state mechanism, not consumer parity.

Implemented D2b-1 API surface: `AnimationPoseRetention::{NoHold,
HoldNativeFinal}`, `prepare_animation_start_with_pose(owner, request, policy)`
returning the existing prepared ticket type, `animation_pose_view()` exposing
only opted-in Playing/HeldFinal entries, and `clear_animation_pose(&PlaybackId)`
for an actual opted-in active or held ID. The clear rejects stale/unrelated IDs;
active clear uses the existing cancellation path, held clear emits no terminal.
These host result types remain outside serde/transport, like D2a. Exact validation
and limitations are recorded in the separate D2b-1 progress entry.

Keep AnimationStart's current serialized shape unchanged. Carry the option and
captured applicability privately in the existing prepared ticket and receipt;
compare the option when checking nonce equality. Same request/nonce with another
option rejects, rather than upgrading a replay. Opted-in starts still obey the
existing tween-zero restriction. Normal clocks for Pawn0..31,
Weapon and Image keep their existing admission and completion semantics. An
opt-in hold request for Pawn0/1/3/4..31, especially Pawn15, Weapon, any ordinary
Image or emote Image is explicitly unsupported and rejects before mutation.
Do not remap Pawn15 to a native body slot. The option is a host API choice, not
a package capability, source callback, clip-name heuristic or extra identity.

Pawn2 is the bounded initial target because the current client gives it the
whole-mounted-image action applicability law. Other client free threads also
visually hold, but have different applicability and remain later work. Narrowing
this first reservation does not remove their eventual parity requirements.
Native producer activation is a separate D2b-2 reservation below, required before
claiming ordinary native actions use the retained state. No existing source
producer opts in as a side effect of D2b-1.
Recommend reviewing/granting only D2b-1 next. D2b-2 below enumerates audited
candidate paths, not an implementation-ready producer grant: an exact trusted
opt-in decision at those producers still needs separate root/source review.

## Exact existing ownership and applicability

The clock stays in `sim/src/session/animation.rs`. Existing identities are
PlaybackId(session/owner/serial/life/body), AnimationBinding(owner, optional
AnimationItem(slot/generation/item), optional AnimationImage(hand/generation/
image/emote)), plus admitted native clip resource/digest/sequence. Use these
without a new epoch, package counter or name-based incarnation.

Retain the existing D1 body-input receipt by Arc as well: actual full Archetype
definition, resolved resource/reference identities and fitting policy. Recheck
that actual definition/input identity for opted-in state. In-place tuning can
change role/socket/fitting policy without being a new item or life; owner epochs
alone must not make an old pose context valid. Reuse the full D1 receipt instead
of a model-only key or a new body-policy counter. NoHold clocks keep their
existing binding law.

For this Pawn2 policy, capture a fixed four-slot ordinary-image snapshot at the
actual producer boundary: `[Option<AnimationImage>; bri_weapons::IMAGE_SLOTS]`.
Each occupied slot must have its actual generation, matching hand index and
`emote=false`. Empty slots are explicit. Compare all four slots for equality
while also applying the existing binding validator, including actual selected
custody where the request binds an item. Owner-only requests do not acquire a
guessed item binding. This catches another-hand/worn-slot changes and same-ID
remounts; item binding independently catches duplicate-slot custody changes even
when the runtime intentionally preserves the same image mount.

Read `image_id(actor, slot)` alongside `animation_image(actor, slot, false)`:
None from the identity query must not disguise an occupied mount whose generation
is unavailable. Reject opt-in admission as unsupported in that case. Unchanged
legacy clocks remain unchanged. Four direct existing queries suffice for D2b-1;
no new public inventory iterator or weapons API is necessary for that step.

Client evidence at the baseline: `app/avatars.rs` builds Pawn2 applicability from
every `WeaponView.images` entry, sorted by hand and joined as hand:image. Session
`weapon_view` enumerates ordinary slots 0..4; therefore the current set includes
worn slots 2 and 3, despite the client's nearby comment saying hands. It excludes
the separate runtime emote store. D2b should preserve this slot domain while
using stronger existing generations. This stronger invalidation on same-ID
remount is intentional authoritative identity protection, not a proven original
source law; require the independent consumer checks below.

Emote and ordinary worn slot3 are distinct stores, even though both can report
hand3. Do not collapse them into a map keyed only by hand. Emote-only changes do
not invalidate an unbound hold through this ordinary-image-set policy. Explicit
existing qualified bindings still obey their own actual item/image validation;
the set does not override that binding law. Future emote producer policy must
identify `emote=true` separately and demonstrate its own producer semantics.

### Audited producer, hand, emote and custody paths

Paths below are relative to `crates/`; these are read-only findings at 84dee79a.

| Existing path | Identity/applicability fact | D2b consequence |
| --- | --- | --- |
| weapons/src/runtime.rs: animation_owner/item/image, animation_binding | Identity reads use existing generations; binding associates the selected item only when its right/left image matches. Worn images can be image-bound without a matching selected item. | Reuse binding validation plus full ordinary-slot vector; do not fabricate custody. |
| runtime.rs: give_at, set_inventory, drop_item, take_item, pickup | Give/loadout allocates existing serial generations; removal clears them; pickup calls give and gets a new custody identity. set_inventory unmounts hands and clears selection. | Bound holds cancel on replacement/transfer even with identical item names and slot numbers. Owner-only holds follow ordinary images, not an invented inventory-wide law. |
| runtime.rs: equip/change_image | Selection changes immediately; image switching can wait for allow_change. Same image/paint may stay mounted; duplicate remount occurs only when existing magazine policy requests it. | Do not infer a remount from a selection request. Item-bound state can cancel before a pending image actually changes. |
| runtime.rs: mount/swap_images/swap_image/unmount | Actual mount allocates animation_generation. Right-hand swap brings left_image; unmount clears hands 0/1, leaving worn slots intact. swap_image can force replacement while retaining selection. | Compare actual mounts, not desired next image. Same-ID forced remount must invalidate. |
| runtime.rs: wear/clear_worn/take_worn | Ordinary slots2/3 use mount generations; replacing even the same worn ID remounts. Tool changes leave worn images alone; death/body paths clear them. | Include these slots in the ordinary set, with explicit occupied/empty identity. |
| runtime.rs: emote/advance_emote/respawned | Separate emote Equipped with its own existing generation and EMOTE_SLOT hand; repeated emote mounts replace it. advance_emote dispatches ToolFire commands, not ordinary state.sequence animation events. Respawn clears it and old animation cues. | No guessed emote image producer or automatic body hold. Ordinary worn3 and emote3 coexist as separate identities. |
| runtime.rs: advance + state.sequence/state.arm/state.gesture | Ordinary slots0..3 advance with their current Equipped temporarily taken out of a.images. sequence emits Image(hand,false); arm emits body thread2; gesture emits body thread3 with existing provenance. | Only a separately opted-in arm/thread2 producer may become eligible later; Image and gesture stay NoHold in this proposal. |
| runtime.rs: PendingCue creation/play_cue | Queue captures only one AnimationBinding today; dispatch allocates provenance serial later and forwards that old binding. | Full set must be captured at queue creation, not recomputed when dispatched or consumed by Session. |
| sim/src/session/weapons.rs: WeaponEvent::Animation | Session admits only events with actual provenance; native_animation maps image_hand to Image(emote=false). Cosmetic cue emission is separate. | Missing provenance must remain unsupported. Never reconstruct authority from cosmetic cue replay or infer emote=true from hand3. |
| sim/src/session.rs: play_thread/schedule_thread | Free body producer has no weapon provenance; native_animation uses owner-only binding. Scheduled free calls execute as current-owner controls at their due boundary. | Existing paths stay NoHold. Later opt-in captures the actual set at this free producer, without pretending a free timer is an item-bound PendingCue. |
| sim/session inventory equip_tool, command boundary and step end | Existing reconciliation runs after direct equip, commands and step; advance_animations reconciles before Natural. Combat/respawn/body/remove paths use existing life/body cancellation or binding invalidation. | Extend the same reconciliation owner to held entries. Audit each actual mutator boundary; do not add hooks to arbitrary package calls. |

The weapons runtime removes the Actor from its world map while advancing it and
takes the current Equipped out of its slot. A future full-set capture helper
must accept that Actor plus the detached Equipped and overlay the current slot.
Reading `self.actors`, or iterating only `a.images`, at emission would silently
lose the current image. Snapshot after an actual unmount must still reflect
the resulting state; overlay is only for a live detached advancing image, not
for an old image passed to a post-unmount root call. Require explicit call-site
review and tests for both cases. A delayed cue stores the original complete
snapshot and forwards it unchanged. Final Session binding AND full-set validation
reject stale origin even when only another hand changed during the delay.

## State and transition contract

Store held entries at the existing clock owner, disjoint from active by
(OwnerId, AnimationTarget). A held entry retains immutable playback/clip/binding,
the opt-in/applicability receipt and native final sample phase. It stores no Rig
copy, node Channels, wall-clock timer or terminal reservation. A separate read-
only pose view can expose opted-in Playing versus HeldFinal with the existing
tick/revision and explicit phase. D2a evaluated active view and legacy anchor
view keep their accepted semantics. Terminal records remain independently drained.

| Event | Active clock/records | Opted-in visual state |
| --- | --- | --- |
| Valid fresh start | Existing Started and normal active anchor. | Playing; owns the same target occupancy from admission. |
| Natural finite end | One Natural at the existing clock duration; active entry removed. | Transfer the same occupied key to HeldFinal, sampled at native final phase through the existing native clamp. Never stretch keys to a dedicated/source deadline. |
| Zero-duration finite start | Existing atomic Started + Natural, with no active entry. | Install HeldFinal at phase0 in that same transaction if opted in. Default leaves no held entry. |
| Loop or pause | Loop never produces Natural; rate0 remains active. | Playing; no fabricated held state or deadline. |
| Valid replacement on same target | Existing Interrupted only if old entry is active, then Started/possible zero Natural for new ID. | Replace held/playing state atomically; replacing held produces no extra terminal. NoHold replacement clears any previous hold. |
| Duplicate request | Existing exact receipt replay returns old ID; evicted nonce stays rejected by watermark. | Never replays a start, recreates a cleared hold or reserves capacity. Same nonce with another retention option rejects. Do duplicate resolution before capturing a new set. |
| Explicit cancel while active | Existing one Cancelled; removed. | Clear, with no hold on Cancelled or Interrupted. |
| Explicit clear by PlaybackId after Natural | Existing cancel_animation remains stale/active-only. Proposed separate pose-clear API validates actual current held ID. | Remove hold and advance existing revision; no second terminal. Stale ID cannot clear a replacement. |
| Valid native root on Pawn2 | Existing active cancellation semantics unchanged. | Clear held target too when this ordinary control is valid; never manufacture another terminal after Natural. Existing producer binding checks precede root. A stale producer cannot clear a new incarnation. |
| Life/body replacement, death, disconnect/remove | Existing cancellation records for active entries remain unchanged. | Clear held entries using actual owner identities/validity; no terminal duplication. Same-player cosmetic rebuild does not invent a new owner epoch. |
| Custody, actual D1 body-input identity or full ordinary-set mismatch | Existing bound active cancellation remains. Opted-in active also checks captured body context/full set. | Clear held entry; current body/clip cannot silently adopt new equipment or tuned body policy. |
| Failed admission or asset replacement | No mutation, nonce consumption, cancellation or partial catalog install. | Prior active/held entries, identities and applicability remain intact. |

Native root is an ordered current-target control, not a playback-specific source
callback. Preserve its existing valid-producer ordering; do not invent a relation
between a root event serial and a previous start serial. A PlaybackId-specific
host clear is the separate mechanism for stale-clear protection.

Catalog replacement must validate held native resource/digest/sequence and the
staged D1 body-input receipt as well as active metadata in the existing single
transaction. Reject incompatible live
held metadata rather than swapping underneath it; caller can explicitly clear
the hold then retry. Preserve valid held entries on an equivalent catalog swap.
Opted-in active entries require the same staged body-input comparison; NoHold
active catalog rules stay unchanged. The incoming BodyPoseInputs::info accessor
is currently private to body_pose.rs, so D2b-1 needs an explicitly reserved
visibility-only change to reuse it for staged read-only checks. No second
validator/cache or replacement resolution law.
Actual body/life changes still clear holds through the existing owner path. No
new model-only cache identity, reference inference or fallback body. Invalid
applicability is reconciled at existing authoritative boundaries. A pose query
must never expose an inapplicable hold; it may omit it read-only before the next
reconciliation, but cannot reclaim capacity or consume records. Admission counts
unreconciled entries conservatively until an explicit existing reconciliation or
clear releases them; queries never evict state to make a request fit.
The revision remains the clock's mutation revision, not a combined world-state
version. A read-only applicability filter can change after external inputs
change, before reconciliation. Callers must query current applicability at the
authoritative boundary or key caches by its actual input identities, not only
tick/revision. Commit always revalidates those actual identities as well.

## One occupancy budget, preflight before mutation

Keep limit128, using the existing MAX_ACTIVE value as a combined occupancy cap:
`K = keys(active) union keys(held)`, with active/held disjoint. A fresh start must
satisfy `len(K) + (target_key not in K) <= 128`. Replacement reuses its occupied
key. The rule applies to every start, including NoHold and zero duration, keeping
the original conservative zero-duration check when no holds exist. Natural
active-to-held transfer allocates no additional slot. Zero opted-in admission
reserves the held slot before Started/Natural. Do not add a second arbitrary
pose cap, TTL, eviction, silent truncation or terminal-backed hold reservation.

Keep the independent result bound `records.len + active.len + 2 <= 512` for
new starts exactly as today. Active entries alone reserve one future terminal.
Natural consumes that reservation while removing the active entry; held entries
reserve zero result slots. Replacement of a held entry may conservatively need
the existing two result slots even if new zero duration completes immediately.
Draining records cannot free pose occupancy; clearing holds cannot free records.
Default behavior/capacity remains identical when no opt-in holds exist. Once a
host explicitly uses holds, they intentionally share capacity with active work;
failure must name occupancy versus result pressure separately.

Prepared tickets retain the existing revision/catalog guard, validated native
clip, retention option and captured applicability. Commit revalidates actual
binding/full set and applicable capacity before any finish, replacement, record
write, nonce update or hold install. Prepare/commit rejection is atomic. Hold
clear/invalidation and pose-visible changes use the existing revision, with
checked admission for the full planned mutation count. Account for held removals
in owner/catalog/rate revision headroom audits; do not introduce a pose revision
counter or an unchecked overflow path. Keep receipt256 and nonce-owner4096
bounds; receipt eviction must not evict or resurrect a held pose.

D2b-1 implementation preserves `revision + K + P <= u64::MAX`, where P is
opted-in active count. K reserves eventual cleanup; P additionally reserves
Natural-to-held before that cleanup. New admission preflights three extra
increments; rate/catalog operations one. Natural/clear/owner cleanup consume
their reserved headroom, with checked increments. This is a derived bound at
the existing revision owner, not a new counter or terminal reservation.

## Production reservations root would need to grant

Root granted the D2b-1 row from 3d44bfb7. The remaining rows are ungranted.

| Independent reservation | Proposed exact paths | Boundary |
| --- | --- | --- |
| D2b-1 host opt-in, bounded state and lifecycle | crates/sim/src/session/animation.rs; session.rs exports only; sim/src/session/body_pose.rs existing info visibility only; new sim/tests/retained_animation_pose.rs; own docs | Four-slot reads use current public weapons identity APIs. New host option/query/clear; old producers NoHold. Existing clock/catalog/cancellation owner handles storage and invalidation; staged D1 resolution reuses the same validator. No other engine paths needed unless a red control proves a missing authoritative boundary; report it to root first. |
| D2b-2 actual producer opt-in, after D2b-1 review | crates/weapons/src/runtime.rs; crates/weapons/tests/animation_identity.rs; crates/sim/src/session/animation.rs; session.rs play_thread/schedule firing only if explicitly selected; sim/src/session/weapons.rs Animation arm only; a new focused sim producer test; own docs | Separate trusted opt-in mechanism and exact producer allowlist must be reviewed before editing. Snapshot immediate/delayed source context correctly. No automatic activation just because thread==2 or namespace string matches. No package hooks/counters/transport. |
| Later composer/motion/consumer work | No paths reserved here | motor/client extraction, full body layers, vehicle provider/frame APIs, dynamic eye/fire and source completion/transport remain independently owned work. |

## Source-free Session red/green plan and consumer requirements

Before D2b code, preserve D2a9/clock15 controls and add a separate Session red
test demonstrating that an opted-in finite Pawn2 loses its final pose at Natural
on 84dee79a. Since the proposed API is absent, first express the behavioral gap
through current active-view/native-final sampling controls; then port that test
to the granted host API. Keep the D2a active view's absence-after-Natural
assertion unchanged; only the test's pose-source adapter moves to the new view.
Record the baseline gap separately from compile-time
absence of the new API. Use invented validated native keys, actual Session
steps, real inventory/images and qualified bindings, not a fabricated held map.

Green controls must prove:

- Default start still disappears; opt-in same native clip reaches final native
  sample and remains held across ticks/outbox drains, with one Natural only.
  Loop, pause/rate, zero, same-tick zero replacement, NoHold replacement, duplicate
  after completion/clear and nonce eviction have the transition outcomes above.
- Actual root clears active and held appropriately. Stale specific-ID clear,
  stale item/image producer, body/life replacement and disconnect/resume cannot
  affect a newer pose. Actual in-place archetype tuning invalidates the opted-in
  body context without fabricating an epoch. Failed prepare, stale ticket and
  invalid catalog preserve
  full active/held/outbox/applicability state. Equivalent catalog retains valid
  holds; incompatible held native/reference/fitting inputs reject until explicitly
  cleared. Missing/stand-in/vehicle-owned providers remain unsupported.
- Empty hands, all four ordinary slots, forced same-ID redraw, another hand,
  worn replacement/removal, pending allowed-change switch, duplicate same-item
  slots, take/drop/pickup, inventory replacement and real life/body transitions
  use actual existing generations. Emote-only replacement leaves this ordinary-
  set hold unchanged; ordinary worn3 replacement clears it. Missing identity for
  an occupied mount rejects opt-in instead of being treated as empty.
- Fill all128 target keys with a mix of actual active clocks and held Pawn2s
  across real owners. Key129 rejects without side effects; same-key replacement
  succeeds when result budget permits; Natural transfer succeeds at occupancy
  saturation; explicit clear releases exactly one slot. Separately saturate
  result pressure and prove held entries consume no future-terminal reservation,
  Natural remains deliverable, and neither draining records nor pose queries
  evicts or resurrects a hold. Unsupported-target/option and capacity failures
  leave their nonce reusable for a subsequent valid request.
- D2b-2 separately proves real weapon arm emission includes detached current
  image, akimbo/worn state, and original queued whole-set identity. Change only
  another mount before delayed dispatch and require rejection even if primary
  binding is still valid. Include post-unmount root without reintroducing the
  old detached image, and no-provenance events remaining unsupported.

Run targeted native Session controls, changed-crate quick and scoped focused on
each separately granted code atom, with Cargo jobs2 and CARGO_TARGET_DIR unset.
No broad tests are needed for this documentation-only checkpoint.

Independent original consumers remain required on exact SHAs routed by root:
Halo's pinned `fbdd03ead1c943229a34de68b0a2e5463ef7c094` passenger handoffs
require authored seat/ready plus sparse reload channels preserving seated pelvis;
the ordinary-set applicability/held key must be checked through actual boarding,
equipment changes and two-axis source aim. No visible mount implies a fire-event
or gameplay muzzle law. Separately vehicle-owned rigs remain outside D1.

AA's same-pin reload timing/consumer notes distinguish 140/30 visual extent,
141/30 dedicated GetAnimDuration, standalone/listen last-phase notification,
dedicated polling, early FinishAnim release and independent Pawn15/tween/fades.
No Natural-to-source-Finish mapping is accepted by a held pose. AA actor PR47
`49104a6` still requires authored-prone/dynamic CameraBone proof; PR54
`def522999e9e9413a9a2c947f5504a8eeb08995b` remains a static old-pin custody
audit to re-evaluate against actual runtime generations. D2b-1 cannot close
either source consumer. Root owns routing, new code reservations and integration;
Maxwell owns every interactive playtest.
