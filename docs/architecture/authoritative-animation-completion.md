# Authoritative animation admission and completion

Design only, from sampler `b37b280f99ed4420650391bb535e1d705ac828ab` on
`codex/authoritative-animation-completion-design`. Root reserves this note,
sim/tests/authoritative_animation_completion.rs, own progress and one seam row.
Production is read-only pending review/grant. Do not integrate the deliberately
red diagnostic alone. This is a bounded first clock stage within
[the existing layer/fitting/host plan](mounted-pose-authority.md), not another
animation graph or a replacement for its shared sampler and body-layer planner.

## Observed blocker and existing boundaries

AA PR38/private3f593e07 records the actual ordinary Session M16 exchange:
12 loaded rounds become31 immediately; after600 more ticks and redraw its
package/native image remains AwaitSourceCompletion. These ticks demonstrate an
absent producer, not a proposed reload duration. Weapon busy.AnimEnd/authority
Finish and pawn channel15 AnimEnd are distinct source clocks. Original selection,
rates, tween and interruption policy remain AA-owned.

Session::play_thread currently emits WeaponAnimation(actor,thread,sequence,None).
session/weapons.rs turns the runtime Event::Animation into the same cue, with
image_hand distinguishing an image's own sequence. Neither retains authoritative
phase or terminal results. Package PlayThread is an effects operation bounded to
0..3, Cue validation already accepts threads below16, and the client free-thread
adapter only interprets its existing slots. Raising one bound does not create
authority, original channel layering, lifecycle or completion.

Combat::spawn_tick is available. WeaponsWorld Actor inventory contains item-id
strings and selected slot; Equipped retains image/state/hand/magazine key, but no
item or actual mount incarnation. A cue's stream id orders cosmetic delivery;
it is not an admitted clip token or a promise of completion. Two distinct slots
can produce identical Animation payloads. Actor-only Ammo and per-ray Tracer
also cannot supply a general accepted-shot action identity.

Validated native Shape/Animation data already supplies duration, looping and
keys. Catalog retains provided native body asset bytes; net/content_identity's
bounded body_rig and presentation/image model readers already locate these
resources. They currently discard full body metadata after static grip sampling.
Registering immutable clip metadata is smaller than implementing mounted fitting,
IK, dynamic sockets or host body matrices. Completion can precede those stages.

## Concrete minimal ownership/API proposal

One Session-owned `AnimationClocks` in new session/animation.rs holds admitted
playbacks and a bounded terminal outbox. It consumes immutable native clip
metadata, never a script-supplied duration, audio event, renderer callback or cue
replay. Metadata identity is package resource/content digest, actual body/role or
image binding and rig/clip generation; a model string alone is insufficient.
Reuse exact native Animation validation. Registration is transactional: absent,
ambiguous, malformed or unbounded assets make the authoritative start unsupported
with a named refusal. Legacy cosmetic cues can still operate without these assets.

Proposed plain types (final names/layout require root review):

```text
PlaybackTarget = PawnChannel(authored channel) | Weapon(item instance)
                 | Image(actual mounted image incarnation)
PlaybackRequest {
  request_nonce, target, clip selector, rate, tween, authored layer,
  explicit clip interval/loop selection when supported
}
PlaybackId {
  session_epoch, owner, spawn_tick, body_generation,
  item: optional { slot, generation }, mount_generation: optional,
  serial
}
ActivePlayback {
  id, requesting package, request_nonce, target, clip resource/generation,
  phase anchor + authoritative tick, current rate,
  validated tween/layer/interval parameters
}
PlaybackResult {
  id, request_nonce, target, clip resource/generation,
  terminal tick/phase, outcome: Natural | Interrupted | Cancelled, reason
}
```

Weapon and Image targets are distinct: a weapon object can remain in inventory
while its image is unmounted. Binding the result to the image lifetime or the
item lifetime must be explicit. Pawn channels are authored logical channels;
channel15 is not silently mapped onto body thread3 or the weapon clock. Reuse
bounded layer admission and the existing native 32-layer sampling limit; do not
silently truncate actions or equate a channel number with a layer index/priority.

Extend the existing typed PlayThread operation with an explicit authoritative
request/options overload and route both existing producer sites into this owner.
The old `(player,thread,sequence,after)` overload preserves its cosmetic contract
and existing scheduling; no legacy cue gains a fabricated source clock. A legacy
start may be tracked only when its actual selected asset and existing playback
semantics are unambiguous; missing metadata is explicit, never a guessed duration.
An authoritative request requires the player mutation capability as well as the
legacy effects permission at the existing authorization boundary. There is no
new network completion command or unrestricted client result operation.

`push(Op)` currently returns an index for `require`, scoped to one Rhai call.
It cannot be returned as a global playback token. The package supplies a bounded,
monotonic request nonce within its current actor action generation, and receives
host `on_animation_started(owner, record)` / `on_animation_result(owner, record)`
hooks through the existing declared-hook/budget machinery. Started acknowledges
the actual admitted PlaybackId. No result can be forged with ordinary Add-On
emit/listens events. A required start must preflight metadata, permissions,
target/incarnation and capacity before package state is committed.

Results never change weapon state or clear busy directly. Package policy compares
the full token and request nonce, then performs existing set_image_state or other
ordinary authorized operations. A pawn natural result cannot release weapon busy.
Interrupted/cancelled results do not imply Finish, refund or unlock. AA supplies
the original policy for each outcome; the engine supplies truthful facts.

## Clock phase and lifecycle

The existing Session step has two visible tick phases: at entry and step_packages
the published Simulation tick is T; step_physics/Simulation::step_with advances
authority to T+1, and step_weapons reads T+1. Preserve that order. Advance clocks
once to T+1 after sync_seated and before step_weapons, beside the future pre-fire
host pose sample already specified in the mounted plan. Enqueue terminal results
there; deliver them at the next step's existing package-hook phase (whose
published tick is then T+1), before on_tick. This is not an extra clock tick or a
renderer delay. Do not advance clocks early at T and then sample stale T phase
for the T+1 shot.

Commands and package play_thread starts retain their actual admission tick T;
they legitimately advance one tick at the following T+1 physics boundary. Starts
emitted later by step_weapons begin at T+1 with zero elapsed phase and cannot
advance again in that same step. No start/terminal event causes a reentrant
callback or retroactively alters that already sampled shot. Budget deferral may
delay callback delivery, never the recorded terminal tick or the phase anchor.
Deliver animation records after the existing death/loadout/spawn hook prefix,
preserving Started-before-terminal order for each id. Thus source death/new-life
policy can invalidate its action before an old natural result is observed; full
identity comparison remains mandatory even when callbacks arrive in order.

Phase comes from native duration/interval and explicit validated playback rate,
with a preserved anchor on rate change. Looping never produces Natural simply
because it crosses a cycle. Pause, reverse, interval endpoints, threshold holds,
initial tween and loop/interpolation conventions need explicit supported
semantics: reject unsupported options rather than approximate them with an
absolute rate, frame-count duration, dense keys or cosmetic tween delay. The
first primitive must state which options it implements and AA must confirm they
express its selected original clip. Source13 interval/threshold/loop constraints
remain independent queued acceptance, not permission to guess these laws here.
Tween changes composition/weight; its relation to clock advancement must be an
explicit authored option backed by source evidence, not assumed to extend or
replace clip duration. No caller-provided completion deadline is accepted.

Admission allocates a monotonically increasing serial before starting. A second
admitted start on the same actual target/channel terminalizes its predecessor
once as Interrupted, including a same-clip/same-tick restart. Replayed sequenced
commands do not restart it; a repeated already-admitted package nonce is
idempotent or rejected by its recorded receipt. Reusing a nonce with different
parameters is rejected. Partial required-operation failure cannot consume an id
as though a new active clip were accepted.

Add per-inventory-slot item generations at the actual WeaponsWorld mutations:
give/remove/drop/pickup/replace/clear/loadout/actor restore. Capture that stamp at
the animation producer, not by looking up the currently selected item when a
delayed cue arrives. Increment actual Equipped mount generation on mount/unmount;
selection can change before a deferred image change and is not that generation.
Two equal item strings, or replacing the same slot with the same item, must have
different item stamps. This identifies ownership; it does not implement original
dropped-weapon magazine transfer policy.

New life, body identity replacement and disconnect cancel their owned clocks;
actual item destruction cancels item-bound clocks, actual image unmount/replacement
cancels image-bound clocks. Inventory-bound weapon clocks do not acquire an
invented holster rule. Body changes need their own generation, since spawn_tick
does not change for every set_archetype; cancelled old results retain the old
identity. Cosmetic outfit rebuilds do not become new authoritative bodies.
Map/session/package replacement invalidates its generation and pending starts;
no old result attaches to the next actor, inventory slot, asset or package.

Terminalization is host-only and once per id. Reserve bounded Started/terminal
delivery capacity before admission/replacement; never silently drop a result
after a package has committed busy. Use existing per-package work shares: budget
deferral retains the same outbox entry; delivery is recorded before a callback
attempt to prevent replay, with script failures reported explicitly. Do not add
unbounded histories or automatic retry of possibly committed callback effects.
An unread result may wait, but cannot naturally finish a replacement action.

## Replication, late join and persistence

Expose a bounded read-only AnimationView from Session, separate from its transient
cue queue: current active tokens, validated resource identities, phase/rate/tween
anchors, revision and lifecycle generations. Checkpoint/delta replication carries
that active state; late join samples the current phase rather than replaying starts
or terminal callbacks. Snapshot/Replica validation rejects malformed or conflicting
state atomically before advancing revision/cursors. Transport needs one protocol
change marker when this field is added, never a direct VERSION edit.

An optional correlation token on the existing WeaponAnimation cue can connect old
cosmetic delivery to an admitted start; it is not the clock authority. The strict
diagnostic below records that the old payload has no such provenance. Root's
review prefers durable active-view authority; Stage1 acceptance must adapt these
assertions explicitly to the approved typed active view, matching actual native
clip/binding, complete generations, phase and terminal results. Adding an opaque
cue id or tick field alone does not satisfy them. Cue correlation remains optional;
do not silently claim the
existing raw cue had item/clip provenance. No terminal history is sent to late
joiners as executable policy. Client duplicate/old-generation views cannot
restart an action or emit gameplay results.

Do not persist active runtime clocks or replay their callbacks after a restart.
Package busy state saved across a restart needs an explicit load/reconciliation
policy; no synthetic Natural result or guessed timer repairs it. New session
epoch and actual package/asset generations make stale ids unusable.

## Bounded implementation reservations to request

| Paths | Concrete purpose |
|---|---|
| sim/src/session/animation.rs (new), session.rs | Session clocks/outbox/read-only view; producer hook, fixed-tick phase and Peer generation/state lifecycle |
| sim/src/session/weapons.rs | Retain actual native producer admission and correlation before emitting existing cues |
| weapons/src/runtime.rs (and lib.rs only if exports require it) | Item/mount incarnation at actual mutations and Event::Animation provenance, no reload/refire policy changes |
| sim/src/session/{inventory,combat,bodies}.rs | Wire actual equip/item/life/body invalidation; preserve source outcome policy |
| package-runtime/src/{ops/effects.rs,script.rs,content.rs} plus existing authorize boundary if needed | Typed PlayThread options, required start preflight, declared Started/result hooks and strict option validation |
| sim/src/session/packages/perform/effects.rs, packages.rs and new packages/animation.rs | Existing operation dispatch and budgeted hook delivery; no generic fake Add-On event bridge |
| net/src/{content_identity,host_setup,dedicated}.rs and client/src/app/session.rs | Register bounded native clip resources in both real host setup routes; no textures/renderer data in Session |
| sim/src/presentation.rs, net protocol/checkpoint/Replica producers+tests, one protocol marker | Optional cue correlation and active-state replication; exact discovered paths before grant |
| client app animation adapter, exact paths after read-only audit | Consume authoritative phase/generations for declared channels; no local gameplay completion or fourth clock |

These are proposed stages, not a production grant. First register/admit/advance
truthful clocks with native lifecycle tests; then package hooks/AA consumer and
active-state replication in separately reviewable atoms. Do not label the first
stage a complete AA fix while its package cannot start the original weapon and
pawn clocks or obtain the result. Coordinate exact constructor/wire footprint
before implementation. No manifests/dependencies/worktrees are needed for this
design; any production discovery must be reported.

## Diagnostic and AA closure

The new sim test writes invented native Shape clips and actual installed packages,
selects their body through the existing operation, invokes real play_thread and
ordinary WeaponTrigger/state-entry producers. Native sampling proves the supplied
finite clip has an endpoint. Six hundred Session ticks leave the package busy;
there is no test-local clock, completion emitter or local magazine simulation.
The unused proposed hook function is explicitly not registered by today's
Behaviour and is not evidence of a working callback adapter. This characterizes
why merely authoring that function cannot close the missing producer.

Two distinct equipped inventory slots sharing one image produce identical cue
payloads even though cue ids differ. Strict ignored diagnostics require admitted
body/life and item/mount correlation from those real producers. Execute them with
--include-ignored to record the red baseline; they are not passing acceptance and
this branch must not enter the integrated include-ignored gate alone. Their
proposed correlation shape is reviewed here before any production API exists.

Post-grant acceptance must enable corrected strict cases and add actual authoritative
Started/Natural/Interrupted/Cancelled hooks: independent weapon versus pawn15,
same-clip same-tick restart, duplicate command/nonce/result, pause/rate changes,
supported tween/interval/loop semantics, same-item slot replacement, deferred mount,
death/respawn/body changes/drop/pickup/disconnect/package reload, capacity and late
join/reliable malformed-state rejection. Clip metadata must come through real
host setup in dedicated and local modes. Unsupported source options stay explicit.

Root routes exact immutable engine SHA to AA PR38. AA changes only its private
consumer to start actual authored weapon and pawn clips with proven parameters,
retain Started ids and release only the matching source-approved result. It reruns
the original immediate31-round exchange/busy guard and firing path without stock
reload timers, including redraw/death and independent channel results. Original
burst/refire/interruption/loadout/drop-magazine/accepted-shot identity remain open
unless separately proven. Source and Halo get exact pins for affected shared
sampling/pose consumers through root; no unrelated fidelity closure is inferred.

## Stage1 exact reservation proposal after root review

Root approved the direction, not a blanket production reservation. Proposed
Stage1 production files are exactly these eleven:

```text
crates/sim/src/session/animation.rs (new)
crates/sim/src/session.rs
crates/sim/src/session/weapons.rs
crates/sim/src/session/combat.rs
crates/sim/src/session/inventory.rs
crates/sim/src/session/packages.rs
crates/weapons/src/runtime.rs
crates/net/src/content_identity.rs
crates/net/src/host_setup.rs
crates/net/src/dedicated.rs
crates/client/src/app/session.rs
```

Existing constructor-only test additions: net/tests/host_setup.rs and
client/tests/trench_dig_flow.rs; weapons/tests/addon_seams.rs has one exhaustive
Event::Animation match needing its new provenance field ignored explicitly.
The search finds four actual SessionContent
literals: these two tests, dedicated.rs and client app/session.rs. No other
SessionContent constructor was found in crates/tools. Proposed semantic tests:
the already reserved sim completion diagnostic (adapt strict assertions to typed
durable view/real terminal records), new weapons/tests/animation_identity.rs and
new net/tests/animation_metadata.rs. Existing docs/progress/seam complete this
proposal; no manifests/dependencies, Cue/Checkpoint/Replica fields, package-runtime
production or wire marker in Stage1. runtime::* is already re-exported by
weapons/lib.rs, so that file needs no edit.

Concrete API outline, all host-side (names subject to grant):

```text
AnimationAssets: validated immutable clip catalog + actual body/image bindings
Session::set_animation_assets(assets) -> Result
Session::prepare_animation_start(owner, origin, request) -> Result<PreparedStart>
Session::commit_animation_start(prepared) -> Result<PlaybackId>
Session::animation_view() -> AnimationView       // native active authority
Session::take_animation_records() -> Vec<StartedOrTerminal> // host-only, once
Session::cancel_animation(full_id, reason) -> Result
Session::set_animation_rate(full_id, rate) -> Result
WeaponsWorld::set_animation_owner(actor, life_epoch, body_epoch) -> Result
WeaponsWorld::animation_binding(actor, hand/slot) -> current incarnation
```

Host metadata builder in content_identity receives current content root/package
list, verified stock Rig, authoritative weapon pack, item-presentation directory
and weapon extras. Dedicated already reads avatar.rig_file, verifies rig_sha256
and validates Rig before shape_grips; retain that metadata there. Local hosting
already owns AvatarAssets.rig, ContentPaths.item_presentation and weapon_extras;
call the same builder in app/session.rs before its background host closure. Both
populate the required SessionContent.animation_assets field. HostSetup::session_with
registers the catalog after install_packages has established actual archetypes,
before any player joins. Catalog replacement checks native resource/digest and
actual body/role/image/item binding transactionally; it cannot substitute another
model supplied by a script. Catalog byte/digest validation and ambiguity tests
belong in the common loader, not duplicated local/dedicated implementations.

Use a Session-owned monotonic epoch allocator for life and body generations;
spawn_tick stays evidence/timing data, never the sole identity. Peer creation,
respawn and actual body/rig/role-binding replacement allocate new epochs even
when two changes share the same published tick. Central set_player_archetype is
in combat.rs, so bodies.rs need not change; install_packages/archetype and asset
registry replacement also reconcile current body identity. Outfit/GPU rebuilds
do not allocate these authoritative generations.

Runtime owns item custody/slot and actual image mount stamps at give_at,
set_inventory, drop_item, take_item, pickup and mount/unmount, including worn
and emote replacement paths. Give/pickup delegates through give_at; set_inventory
is explicit replacement, so equal strings/reordered slots cannot preserve old
tokens by string equality. This records new custody/incarnation and invalidates
old actions; it does not invent original magazine-transfer or holster policy.
Do not add fields to Drop/projectile wire views for this stage. Increment mount
epochs for real changes, not the temporary take/reinsert of Equipped while
advancing its state machine. Rejected operations allocate no new identity.

Event::Animation adds private-runtime producer provenance: owner epochs plus
actual item/mount stamps and a producer-start serial, including body arm/gesture
events originating at an image. image_hand alone is not that origin. Change the
existing private animation helper/callers in runtime.rs to pass the actual actor
state while it is removed from the actor map; do not look it up later in Session
after inventory changes. Session stamps its owner epochs at real transitions,
checks captured versus current binding before admission, and reconciles actual
runtime incarnation changes before natural-end advance. Native command/step and
trusted inventory boundaries reconcile cancellation before exposing updated
active state. All native producers use the same prepare/commit owner; transient
cues keep their current fields and ordering.
PendingCue must retain the origin when a delayed state cue is queued; resolving
the current item/body when it finally emits is not provenance. Its playback start
still occurs at actual admitted emission time, not the earlier queue time. Keep
the runtime's separate worn-image/emote identities explicit rather than deriving
one from equal numeric hand/slot values.

Preparation is pure and rejects unknown/mismatched assets, stale/reused nonce,
unsupported interval/tween/rate/layer and insufficient active/outbox capacity.
Commit rechecks the binding/catalog revision and atomically reserves Started plus
terminal capacity, including old Interrupted plus new zero-duration Started/Natural
when replacing at the same tick. No predecessor is stopped, nonce consumed or
serial published on rejection. Same nonce/identical admitted request is an
idempotent receipt; same nonce/different request and stale generations reject.
Initial scope supports full native clip interval, positive forward rate, explicit
pause with phase-preserving resume, native looping and an explicit zero-duration
natural endpoint. Nonzero source tween/sub-interval/threshold/alternate loop laws
remain Unsupported until separately implemented from evidence; carrying an
unsupported option must not admit a timer with guessed semantics. These limits
mean Stage1 alone cannot be advertised as the original AA completion fix.

The inspected required-operation boundary first calls Perform::check, then
commits script state, then performs operations. On a later required failure it
restores state only if no reentrant operation wrote the namespace meanwhile.
Stage2 must integrate prepared capacity/binding tickets with that real path and
test failing prerequisites/multiple starts/earlier binding mutations/reentrant
operations. Merely adding a late error or relying on conditional rollback is
insufficient. Both capabilities must be vetted before any state change. Stage1
has no package hook/call-state mutation and makes no claim to close this boundary.

Stage2's audited channel gap is concrete: app/mod.rs AvatarThreads has four
entries and play_free_thread only accepts0/1/3; app/avatars.rs treats2 separately
and drops other threads. app/frame.rs restarts image sequence0 from a cosmetic
cue; WorldItems owns that local restart clock. The pure body-layer planner accepts
an action slice and is not capped at four, but current app adapters do not deliver
pawn15 or authoritative phase to it. Stage2 therefore needs actual generation/
phase-driven adapter work in those files and WorldItems, alongside package hooks
and net Checkpoint/delta/Replica. Their production paths remain ungranted and
must be pinned separately. Session's native animation_view accessor does not
silently add a network field or replay callbacks to late joiners.

## Fitting/socket work remains next

Read current Avatar::fits, fit_hands, finish_pose, mount_node, mount_socket and
mount_action against mounted plan Stages B/C. Boundaries remain suitable: extract
plain BodyFits and existing generic raise/support, then world seat reach after
finite inverse model/root transform. Preserve right-before-left reach, authored
hand preservation, actual resolved role/cache identity, thread2 support liveness
versus absolute final-key hold, and acted/unacted transition boundaries.

Socket extraction then preserves hand_turn, fitted held-hand correction, offset
and tool-only action multiplication order. Reference-tool fallbacks affect sockets,
not borrowed body deformation. First-person eye/mount metadata is not gameplay
origin policy. The animation clock does not fix the retained mounted Session
origin discrepancy; its immutable assets/state become inputs to that existing
shared composer and pre-fire host phase. AA prone double rotation and Source
endpoint/threshold/loop cases stay explicit checks for the subsequent pose stage.
