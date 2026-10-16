# M16 admission and observation: bounded design decision

Proposal only, based on runtime f937a1632bf002153cd30f86423f15aa237d2813,
joint proposal 5948567ea19276ba4bf7d8658e4aa281b81133c7 and AA consumer
2d977b86f579f02cf2d775038f11de6e41a08a16. Root requested read-only design
closure; this document grants no production paths. It narrows the earlier
candidate table. AA source completion policy is still a separate prerequisite
for turning the strict gameplay failure green.

## Choose one prepared operation, not general rollback

Proposed typed operation: begin a native animation on the current ordinary
right-hand weapon, optionally accompanied by a package-computed loaded-round
write and image-state write. Names are provisional; the contract is concrete.
The package chooses eligibility, spare exchange, desired rounds and state.
The engine does not choose a magazine, apply plus-one, or interpret reload.

The operation contains resource/sequence, existing positive request nonce,
optional expected/desired loaded rounds, and optional desired image state.
The server derives owner, selected inventory slot, actual item custody and
ordinary image binding from a trusted original command context. Namespace is
server-scoped to the owning package, not supplied to impersonate another package.
Only explicit Weapon(actual slot), NoHold, rate1/tween0 is supported initially.
Resource must belong to that package's authorized dependency/provider closure
and actual weapon binding. Pawn15, body hold and emote are unsupported here.

For this operation's call, allow one root package, bounded package-state edits
and exactly this one world operation. Reject nested package parts, other world
operations, entity-variable changes and emitted package events before any
mutation. Existing ordinary calls keep their current semantics. Do not silently
upgrade `require` into a general transaction. The operation is always required.

Commit order:

1. Run the script against its existing state snapshot. Its return value is an
   operation handle, never a committed playback ID. Validate script/state budget,
   capability, the restricted call shape and original command context.
2. Prepare the existing native admission and a bounded weapon patch. Validate
   expected rounds against the exact selected instance; reject out-of-range
   desired rounds rather than relying on name-based clamping. Resolve image
   state index now. Prepare ammo notice capacity and existing clock/record/
   revision reservations. Require the logical target vacant; replacing an
   action is unsupported until source interruption arbitration is qualified.
3. Revalidate both prepared identities immediately before commit. Commit the
   native admission first, while no script, hook or external observer can run.
   On any admission failure, drop the patch and uncommitted package state.
4. Apply the already validated rounds/image patch and publish package state
   using an infallible internal commit path, then publish the prepared ammo
   notice. No call to an arbitrary fallible `Perform` remains after admission.
   Image-state entry remains deferred to ordinary weapon advancement, as today.
5. Deliver Started later at the fixed observation boundary below. That is the
   asynchronous accepted acknowledgement carrying the actual PlaybackId. A
   rejection is the existing command diagnostic/error, with no Started/result
   record and no extra rejected-ACK queue. The script's speculative state is
   not kept. AA must not rely on an in-script return value to unlock or swap.

This makes the immediate source exchange and admission one indivisible Session
action, in the same authoritative tick. It does not claim arbitrary world
rollback. The existing `set_rounds(item-name)` is insufficient: duplicate rifles
require a prepared write to the captured actual slot/instance. A weapons-owned
prepared patch must validate before the clock commit and apply without another
lookup/failure/callback afterwards. If that invariant cannot be implemented in
the reserved paths, stop for review; do not fall back to refund after failure.

Duplicate/stale nonce rejects this entire action before package-state or ammo
writes, even where the trusted-host clock API can return an existing receipt.
Do not reapply the patch on duplicate admission. Reuse existing receipt and
watermark rules; do not allocate another sequence/generation counter.

## Preserve command origin, not a current lookalike

Capture actual single AnimationBinding when an ordinary image command is queued,
including commands produced while Actor/Equipped are detached. Use the live
Actor and advancing Equipped at that point; PendingCue's existing captured
binding is forwarded unchanged for a delayed command. An unmount command keeps
the old image's origin, never a newly drawn image. No whole-mounted-set capture.

Carry that context with ToolFire through the image-command path, run_command,
call_package/run_package and OpCall. A plain typed/chat command or unrelated
hook has no such authority and cannot use this initial operation. Do not make
`from_image: bool` equivalent to qualified provenance. Existing non-animation
commands may continue without a binding; the new operation rejects them.

The ordinary light-key path must capture the current mounted weapon when that
input becomes its image command, then use the same context. Validation compares
life/body, custody and mount against the actual original binding. Same content
name, same slot after replacement, and AA's diagnostic generation cannot repair
stale provenance. Emote3 cannot impersonate ordinary worn3/right-hand0.

## One bounded outbox, at-most-once observation

Keep one clock-owned payload per AnimationRecord. Add private host-pending and
package-pending delivery bits, plus immutable owning-package routing metadata
from accepted admission. Active routed playbacks keep that same ownership for
their future terminal. Route metadata is bounded by existing active/record
limits, with no independent event queue or numbering system.

`take_animation_records` returns each host-pending record once and clears only
that bit. Package delivery clears only its bit. Retire a record when neither
consumer needs it. Unowned records behave as before. Both pending and partially
consumed records count in the existing 512-record resident budget; preserve
existing terminal reservations and revision headroom. An undrained host can
therefore backpressure new admission just as today; do not evict a terminal to
keep a package running. Register the declared observer before accepting a routed
request; absent/disabled observer rejects admission.

Choose one observation pass at Session step end, after existing binding
reconciliation. Take the eligible record set at pass entry in outbox order.
For each, mark its package delivery attempted before invoking the hook. Hook
state writes are budgeted; world operations, nested calls and emitted events
are rejected for this observer. No recursive admission or dispatcher recursion.
New records produced after pass entry wait for the next pass. Started precedes
that playback's terminal, including zero-duration Started+Natural in one pass.
Admission from an input command outside step is observed at the next step end;
it is never synchronous in that script invocation. Authority tick and delivery
tick are distinct fields; delayed delivery cannot retime the clock.

Hook failure drops its speculative state, reports a diagnostic and counts as
attempted: no automatic retry or silent completion. The package remains busy
unless its independently authorized policy changes it; a failed observer never
refunds ammo. This is explicitly at-most-once observation, not durable reliable
messaging. Script errors are not grounds for keeping an unbounded retry queue.
The observer records opaque ID/nonce/target/outcome and current applicability.
It must match exact admitted token before changing action-specific state.
Stale terminal observations may be delivered with applicability=false for
diagnostics; they cannot be presented as completion of the current weapon.
Compute this flag from actual current binding and the existing owner/life/
namespace nonce watermark: a later admitted action makes an older result
inapplicable even if it used the same mounted item. No new action counter.

Packages currently install only before players join. Do not invent live package
reload support or an incarnation counter. Any future package replacement must
explicitly abandon old delivery routes before installing a new instance; that
is outside this initial slice. Session teardown discards its own bounded state.

NativeNatural remains observational. AA's first observer stores evidence only;
it does not clear reload/fire busy. Local/listen permitted callback eligibility,
dedicated count/Sleep law, fire completion and interruptions need AA's exact
source mapping and a later reviewed policy grant. No second swap, refund,
completion command, timer, or Pawn15-to-weapon remap is introduced.

## Falsifiable controls and exact proposed reservation

| Control | Required observable result |
| --- | --- |
| Exhaust clock capacity, bad image state, wrong expected rounds or script error during reload intent | Loaded12, original spares/state/image, no Started/new playback/Ammo notice; error reports admission refused. No compensating refund. |
| Valid partial reload | Exactly one immediate12->31 swap and matching spare11; exactly one Started ACK with actual binding/ID; repeat nonce changes nothing. |
| Same rifle in two slots, or forced same-ID redraw after queued command | Old command rejected; neither replacement magazine nor busy state changes; a fresh ordinary request succeeds. |
| Hook tries another request, world write or nested package call | Observer state discarded, diagnostic once, no recursive dispatch/new playback/world mutation. Host still sees original records once. |
| Hook throws; then another step | No retry, no ammo change/refund or implied Finish; finite record retirement after host drain. |
| Host drain before/after observer; zero-duration clip | Both consumers see Started then Natural once, independent of drain order; resident record bound and terminal reservations hold. |
| A terminal for action A arrives after new life/mount/action B | Original ID and applicability=false preserved; B's rounds/busy/token unchanged. |

Minimal proposed production set, all still ungranted:

- `crates/package-runtime/src/ops/player.rs`, `ops/list.rs`, `script.rs`,
  `content.rs`: one typed action, binding and one declared observational hook.
- `crates/weapons/src/runtime.rs`: prepared actual-instance patch and original
  ToolFire provenance at its emit sites; reuse PendingCue binding. No identity
  allocator/save schema changes, general producer activation or new module.
- `crates/sim/src/session/blueprints.rs`, `weapons.rs`, `packages.rs`,
  `packages/perform/mod.rs`, `packages/perform/player.rs`: context plumbing and
  restricted prepared action commit/observer enforcement.
- `crates/sim/src/session/animation.rs`: private routed record envelope and
  admission ownership; existing public serialized request remains unchanged.
- `crates/sim/src/session.rs`: capture/forward actual current binding only for
  ToggleLight's image-command branch, plus exactly one observation-pass call
  after existing end-of-step binding reconciliation. Other direct tool-key
  calls keep the existing unqualified helper and cannot use the new operation;
  no other step-order change.
- Focused package authorization tests, weapons actual-instance/provenance tests,
  and one new Session admission/observation integration target containing the
  controls above. AA alone updates its private ordinary consumer/policy/builder.

No script_world, manifests, protocol/client, renderer, body pose or retention
path is needed by this design. Use a small hook argument builder inside existing
packages.rs rather than expanding a general public player-identity view.
Before granting code, root and AA must accept the restricted action shape and
at-most-once observer semantics. The next acceptance is actual ordinary package
admission/observation with atomic exchange; full source completion remains red
until qualified policy is consumed by that same scenario.
