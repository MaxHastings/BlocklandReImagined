# v0.3.2 M16 consumer-first joint contract

Status: read-only/docs only from accepted `f937a1632bf002153cd30f86423f15aa237d2813`.
No package, producer, motor/client, wire or renderer production grant. Root owns
the next exact path reservation; AA owns original source policy and its private
consumer. Do not expand D2b-2 merely because D2b-1 is tested.

## Evidence and the actual consumer frontier

Accepted engine mechanisms: Stage1 admits verified native clips with existing
owner/item/image identities and bounded Started/Natural/Interrupted/Cancelled
records. D2a evaluates active phase at the existing Session tick. D2b-1 retains
explicit trusted-host Pawn2 final poses only; old producers remain NoHold.
Reviewed weapons allocator input9f233b3f is present as combined code87390ecf,
with receiptf937. Its schema4 restore/trajectory controls pass. These facts do
not create a package admission/result API or source completion endpoint.

Pinned private source inspected at `09e8dbcf015588b94bbdfcc3c3c883efc7c86fe2`:
`aa25/policy/m16_server.rhai`, `aa25/tools/build_m16_policy.py`,
`aa25/tests/m16_package_ammo.rs`, and `aa25/notes/m16-package-completion-handoff.json`.
The policy owns sixteen spare magazines, performs immediate source exchange and
locks the actual image in AwaitSourceCompletion. Its existing per-player
generation is diagnostic package state, not an authoritative item/body identity.
The builder deliberately declares the candidate unplayable and leaves stock
reload_ticks inert. Its shared weapon package supplies ammunition/image states,
not a declared native reload animation provider.

The old real-Session fixture installs packages and weapon definitions, static
body_grips and image barrels. It does not install AnimationAssets/D1 bindings,
request a qualified playback or consume authoritative animation records. Native
resource sampling receipts in other AA tests are not this Session consumer.

| Mechanism | What the inspected private consumer actually uses | Evidence boundary |
| --- | --- | --- |
| Ordinary input and package hooks | ToggleLight reload, WeaponTrigger fire, equip/holster, death; real on_spawn/on_tick and image command callbacks | Current committed ordinary Session scenario on f937, private 2d977b86. |
| Native ammunition/image state | set_rounds, set_reserve, set_image_state; observes actual magazine rounds and one-ray Tracer cues | Immediate exchange and strict busy diagnostic, not full original firing/refire or accepted-shot identity. |
| Static host grips/barrels | Existing body_grips/set_body_grips and explicit image barrels | Static setup only; no dynamic eye/fire or current evaluated-body-pose parity. |
| Native resource registration | Absent from the old fixture; current f937 runner explicitly registers body/rifle metadata and attaches a verification-only rifle model identity | Setup uses real converted resources; metadata availability alone admits no source playback. Not a promoted content package. |
| Stage1/D2a clocks/results | No clock request/result hook in the policy; no clock consumption in the old fixture | Rust clock tests/resource tests alone do not close the package path. |
| D2b-1 final hold | Not requested; Pawn2 host-only opt-in is unrelated to weapon reload release | Do not propose retention activation as this consumer's next dependency. |

AA's current f937 runner reports ordinary paths reproducing: accepted single shot
12 ->11 then fire busy rejects reload; fresh partial reload12 ->31 once then
busy rejects firing and persists across holster/redraw. Its strict reload-then-
fire expectation fails at gameplay, not a hand-called completion. The completed
rerun reports resource1 pass, ordinary observations1 pass, strict acceptance
0 passed/1 failed, and scoped Clippy pass. It registers actual native metadata
but observes zero active playbacks/terminal records. The immutable private pin is
`2d977b86f579f02cf2d775038f11de6e41a08a16`, with
`aa25/tests/m16_reload_consumer.rs`,
`aa25/notes/m16-reload-consumer/f937/run.json`, `strict-red-report.json` and
`v032-owner-handoff.json`. The strict assertion is
`strict_red_ordinary_reload_completes_and_permits_a_new_shot`, emitting
`AA25_RELOAD_CONSUMER_NOT_COMPLETE`; saved stage exits are [0,0,101,0].
This is AA's receipt, inspected read-only rather than rerun in this lane.
Root must jointly review that packet, resource setup and host/remote plan before
approving the next code slice. No source-complete or playable claim follows.

## Source contracts the engine must not collapse

Read-only private source receipts at `fbdd03ead1c943229a34de68b0a2e5463ef7c094`:
`aa25/notes/2026-10-16-native-reload-timing.md`,
`2026-10-16-reload-clock-consumer.md`, `reload-clock/consumer-contract.json`,
plus the committed m16-package-completion-handoff.json inspected above.

- Normal accepted reload exchanges magazines immediately. Partial example:
  loaded12 ->31, first fullest spare returned11 through plus-one loading. End
  notification does not exchange again, refund or create ammunition. The
  existing source candidate's strict redraw/death diagnostics are not complete
  original switch/pickup/correction arbitration.
- Weapon Reload has141 keys at30fps, rate1, tween0. Visual last sample is140/30;
  dedicated GetAnimDuration returns141/30, stored as binary32 plus tween.
  Native finite animation clamps/clears rate and may notify AnimEnd; source
  standalone/listen authority can take that permitted callback to Finish.
  Dedicated authority instead enters inherited Begin and polls Sleep's stored
  remainder, then calls AnimEnd(0). Delta partition, float stores, strict half-
  delta comparison and the actual Begin processing point matter. Diagnostic
  ticks560/564 are not engine deadlines. Do not retime visual keys or hard-code
  4.7s from a request timestamp.
- FinishAnim is a wait. An early stop can release it without Natural AnimEnd.
  Stage1 Natural alone is not an accepted source busy completion. AA must name
  the selected authority route and show the exact permitted mapping before the
  package can release busy. If only a listen/standalone mapping is qualified,
  dedicated remains explicitly unsupported; do not pretend one route is both.
- Pawn15 is independent: StdLgRld/PrnLgRld, rate1, requested .05 tween, subtree
  Spine02 and separate weights/notifications. A first admission can force tween0
  when previous sequence is None. Its callback cannot finish the weapon. Exit
  fades are not added clip time. Source fire/burst/refire and interruption paths
  also need AA revalidation for the shoot-before-reload scenario.

## Smallest useful next slice, conditional on the AA packet

Recommend a single-weapon **owned native admission/result observation bridge**,
not body-retention/motor expansion. It should let the real package and ordinary
inputs produce a qualified native weapon playback and observe its actual result.
An observation checkpoint may remain busy: it is not a successful reload-cycle
acceptance until the exact AA source mapping also releases it correctly.

Preserve AA's explicit verification setup and promote faithful authoring only
within its granted private paths: declare the verified rifle clip
resource and its current native image/provider binding, register it through the
ordinary/common metadata owner or faithful Session setup, and explicitly author
which real fire/reload state asks for it. The production candidate has no such
sequence/provider; the current verifier supplies metadata but no requesting
producer. Do not invent an engine fallback to a stock rifle, use the
soldier body rig for a weapon clock, or call private Rust completion from the
test. If existing metadata setup suffices, that prerequisite needs no new engine
mechanism. Record which actual native Started/result the ordinary path produces.

Initial bridge eligibility should be exactly one owning server package, actual
living owner and selected ordinary right-hand weapon instance, verified native
resource/sequence, rate1/tween0, and an explicit logical Weapon(slot) request.
Derive actual AnimationOwner/AnimationItem/AnimationImage from the authoritative
producer; scripts do not assert generations or rig hashes. An Image(hand0)
native visual record is not automatically a logical Weapon(slot) completion.
Reuse PlaybackId and existing request namespace/nonce for correlation and replay.
An existing package action number may be an untrusted nonce, never a replacement
for actual custody/life/mount identities. Add no engine counter or clock.

The source package may consume a native result only for its exact current opaque
token and qualified action. Generic result delivery must not itself mean Finish.
AA implements its source mapping after validating route, target, lifecycle and
normal/interrupt conditions; malformed/stale/wrong-target/duplicate results never
unlock a newer action. No package-callable finish operation, guessed timer,
audio/cosmetic cue or pawn15 remap. Other producers/targets remain unsupported
by this initial adapter, while existing Rust host APIs stay unchanged.

If AA's exact failing path requires a dedicated source boundary beyond native
Natural, stop before approving a release callback. Specify that boundary's
source start/poll/float/interrupt law jointly, then propose an extension at the
existing clock owner. This document does not design a second timer or silently
substitute the native endpoint. A bridge that reports qualified NativeNatural
is a consumed-mechanism checkpoint, not proof of dedicated source completion.

## Two transaction/ownership seams that need explicit decisions

Rhai operation functions return queued operation handles. `require` preflights
required operations before the call's state is kept, but perform-time failure
does not undo arbitrary preceding world operations. Therefore a proposed
`request_animation(...)` must not claim to return a committed PlaybackId during
script evaluation, or claim that `require` alone makes magazine exchange atomic
with admission. Before a code grant, define prepared admission and accepted/
rejected acknowledgement ordering for this one-action call, including capacity,
binding changes and script errors. A rejection cannot silently commit a swap,
then invent a refund or unlock. Source failure arbitration stays AA-owned.

Do not let a new package dispatcher drain `take_animation_records` and steal
host evidence, or replay cosmetic cues to recover results. Prefer one existing
clock-owned record envelope with bounded route/delivery state: preserve host
drain behavior for unowned records, route only to the owning declared package,
and deduplicate with existing playback/outcome identity. Before granting code,
close the budget/ack rule for simultaneous host inspection and package delivery,
zero-duration Started+Natural, reentrancy, disabled/failing hooks, stale owner
and overflow. Reuse the existing bounded admission/reserved-terminal accounting;
do not add an unaccounted copy queue, new sequence counter or second clock.
Callback observation and applying a source completion policy are distinct steps.

## Producer audit and which detach work is actually needed

`WeaponsWorld::step` temporarily removes Actor from its map and takes the
advancing Equipped out of its slot. Existing `animation_binding(a, Some(e))`
captures that current image and selected item correctly. A whole ordinary-set
capture that reads only self.actors or a.images loses the current image; it must
overlay the live detached Equipped. Post-unmount old Equipped must not be
reintroduced. PendingCue currently captures one binding at queue creation;
whole-set applicability, when actually required, must be captured there and
forwarded unchanged, not reconstructed later from current equipment.

For this initial logical weapon consumer, existing actual item/image binding is
the first relevant identity. Do not require Pawn2's whole-mounted-set retention
policy merely to admit/observe a weapon reload. If the source demands whole-set
invalidation or body composition, add that independently after its real red
case. Ordinary slots0..3 include worn2/3; separate emote3 is not ordinary worn3.
No automatic Image/emote/Pawn15-to-body mapping is authorized.

There is an earlier relevant provenance gap: ToolFire dispatch passes actor,
command and hand, then addon_tool_fire/run_command loses the concrete weapon
origin. OpCall currently contains package/caller/tick, not a qualified source
binding. A future admission adapter must distinguish a current ordinary light
request from an old queued image callback now observing a different identical
item. Capture original context where it exists, validate it at admission and
delivery, and reject unsupported context. Do not substitute the current item's
name/slot or AA's diagnostic generation. Limit any proposed Runtime change to
this consumer-proven gap; do not bundle general D2b-2 activation.

## Candidate path reservations for joint review, not a grant

| Responsibility | Candidate paths | Required gate before code |
| --- | --- | --- |
| Private ordinary consumer + real native metadata | AA's m16_package_ammo/new ordinary-input runner, m16_server.rhai, build_m16_policy.py and verified reload resource manifests | AA exact committed f937 receipt and source binding, no engine file modification or fake completion. |
| Typed package request and explicitly enabled result observer | crates/package-runtime/src/ops/player.rs, ops/list.rs, script.rs, content.rs; focused package authorization/schema tests | Agree one-weapon parameters, existing player/resource ownership authorization, operation-handle versus admission acknowledgement and unknown-target rejection. No new broad capability or finish command. |
| Prepared commit and owning-package result route | crates/sim/src/session/animation.rs; session/packages.rs; packages/perform/player.rs and perform/mod.rs; proposed packages/animation_hooks.rs; script_world.rs only if an actual opaque-view adapter is necessary; focused Session/package tests | Close transaction and bounded dual-consumer delivery before selecting exact paths. Existing clock/defaults/identities remain owner. No speculative public generation inventory API. |
| Concrete ordinary weapon callback origin | crates/sim/src/session/blueprints.rs addon_tool_fire/command; session/weapons.rs ToolFire arms; crates/weapons/src/runtime.rs only for proven detached/queued source capture | AA red case proves missing context. Agree exact current-light versus queued callback eligibility; existing counter/binding reuse only. This is not permission for every weapon producer. |
| Source-specific completion | AA source policy first; existing sim animation owner only if a separately reviewed generic boundary mechanism is needed | Exact selected authority/fire/reload/interrupt mapping. No NativeNatural-as-Finish assumption or request-time N/FPS deadline. |

This table is a candidate dependency map, not a request to grant every row at
once. Root should choose the smallest row set after the AA packet identifies
the first blocked ordinary transition and which current mechanisms it consumes.
No root manifests, network/client, body motion, vehicle or renderer path is
reserved. Any extra path needs concrete red evidence and separate review.

## Consumer checkpoints and acceptance

1. **Baseline, no workaround:** exact engine/private pins, actual package load,
   ordinary trigger/light/equip/death, complete rounds/spares/busy/native-state/
   tracer observations. Separate shoot->reload and fresh-partial-reload paths.
   Retain the strict executable failure at reload completion or resumed fire;
   never enable a user finish command or wait a guessed release duration.
2. **Consumed native mechanism:** same scenario authors real verified resources;
   identify actual qualified start/evaluated phase/result, not injected tokens.
   Show whether the package receives it through the proposed bridge or a host
   observer only. Failed/unsupported admission must report explicitly before
   claiming an accepted action. D1/D2a/D2b labels alone are not evidence.
3. **Source-approved gameplay:** accepted reload performs exactly one immediate
   exchange; busy rejects fire and repeat exchange until the exact allowed
   source completion. Completion transfers/refunds nothing. Then ordinary fire
   spends the correct round(s); fire busy/refire needs its own source mapping.
   Wrong target, pawn15, stale body/custody/mount, duplicate and old callback
   cannot unlock. Holster/redraw, interruption/death/pickup are separate source
   cases, with unsupported arbitration explicit rather than made-up parity.
4. **Host/remote handoff:** AA supplies ordinary listen/host and remote-player
   scenarios and expected authoritative versus presentation observations. Root
   owns any transport grant; do not claim host-only Rust receipts prove remote
   animation/camera parity. Maxwell alone performs interactive playtests.

No broader dependent mechanism is justified until a checkpoint consumes the
previous one and its actual failure names the next need. AA's tree material
packet689eda57 remains queued and unrelated to this contract.
