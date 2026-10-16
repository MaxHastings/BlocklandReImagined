# Session clock and weapon patch: smallest host composition

Proposal only, on accepted immutable `db12842aacdb9bebec72ae5db6b267f6273875e0`.
Root requested docs only. AA compatibility review is private
`e4402c420d0ead7a63ef31fd7916aa2135bad7df`; A1/N1 remain OPEN/red.

## Decision and exact reservation

Recommend one synchronous trusted-host Session method in the existing clock
owner, rather than another prepared ticket, general transaction object or closure
API. It is independently testable with an actual Session/native clip before the
package bridge. The eventual narrow package adapter should call this method,
after its own authorization and speculative-state checks, rather than duplicate
the split-borrow logic. This method alone does not publish package state.

Proposed signature in `crates/sim/src/session/animation.rs`:

```rust
pub fn start_weapon_animation_with_patch(
    &mut self,
    owner: OwnerId,
    request: AnimationStart,
    expected_rounds: Option<u32>,
    rounds: Option<u32>,
    image_state: Option<&str>,
) -> Result<PlaybackId>
```

Exact proposed production edit: add only this method to that file's existing
`impl Session`. It already has access to private `animation_clip`,
`AnimationClocks::prepare/commit`, fields and duplicate receipt marker. No
visibility changes or modifications to existing functions are needed. No
session.rs export is needed for an inherent Session method. Add only
`crates/sim/tests/weapon_animation_patch.rs` plus own documentation for tests.
The accepted weapons helper stays unchanged. These are proposed paths, not a
production grant.

Inlining everything in future packages/perform/player.rs would require exposing
clock internals to a sibling module or duplicating validation. This single host
entry point keeps that boundary owned by animation.rs. Do not add a separate
public PreparedSessionAction or general mutation/rollback framework.

## Ordering and borrow proof to implement

Inline front checks accept only Weapon(slot), a matching bound inventory slot,
ordinary right-hand image0 (not emote), rate1/tween0 and a vacant logical target.
Resolve the actual finite native clip through existing `animation_clip`; it
already checks the living peer, current admission binding, actual model mapping
and verified clip catalog. Reject looping clips for this initial normal action.
NoHold is fixed; no Pawn2 applicability or whole-mounted-set capture.

The implementation then has this shape (illustrative body, not compiled code):

```rust
// Inline shape/vacancy checks above; no caller script or callback here.
let clip = self.animation_clip(owner, &request)?;
let tick = self.simulation.state().tick;
let (animations, weapons) = (&mut self.animations, &mut self.weapons);
let patch = weapons.prepare_weapon_patch(
    ActorId(owner), &request.binding, expected_rounds, rounds, image_state,
)?;
let prepared = animations.prepare(
    owner, request, clip, AnimationPoseRetention::NoHold,
)?;
ensure!(prepared.duplicate.is_none(), "Duplicate weapon action");
let id = animations.commit(prepared, tick)?;
patch.commit();
Ok(id)
```

Read everything requiring `&Session` before splitting fields. After acquiring
the guard, call only the independently borrowed clock and consume/drop the guard.
Do not call the whole-Session `commit_animation_start`, query or mutate other
Session systems, advance a tick, or invoke a closure/script while the guard lives.
The request binding is checked by the weapons helper without being retained in
its return lifetime, so moving request into clock preparation remains possible.
No unsafe code, new lifetime ticket or clock visibility relaxation is required.

Preparing the weapon patch before clock preparation is deliberate: a real full
clock outbox can refuse while a valid patch guard exists, exercising abort rather
than relying on a constant external-clock stand-in. Native binding/clip refusal
and patch expected/range/state/event-capacity refusal leave no gameplay writes.
Clock prepare refusal or duplicate receipt drops the patch with no ammo/image/
queued-notice, active, record, revision, serial or nonce-watermark change. A
reserved event-vector allocation is not a gameplay write.

Clock commit keeps its existing revision/catalog recheck, active/held128,
records512 and terminal/revision reservations. Current `commit` performs every
Result-returning check before its first `finish`/mutation. Vacant target prevents
replacement/interruption here. No hook runs between prepare and commit, so this
wrapper creates no normal interleaving that makes its own prepared ticket stale;
do not invent a fault callback merely to manufacture one. Capacity refusal is
tested through real clock preparation, not mislabeled a commit-time race.

Once clock commit succeeds, only the accepted infallible patch commit and return
remain. No late validation, lookup, arbitrary Perform, refund or cancellation.
Both states are visible together after this exclusive synchronous method returns.
Started has the authority tick captured above. Ammo notice is queued once;
existing projection/delivery timing is unchanged. This does not promise network
delivery order or package ACK delivery. A zero-duration finite clip retains
existing Started+Natural order in the same tick; neither outcome executes policy.
Ordinary allocation/panic recovery is not a new transactional guarantee.

## Concrete Session red/green control

Use one source-free native fixture with separate body-zero and rifle-finite
Shape resources, validated bytes/digests and explicit real weapon model metadata.
Never use a soldier body clip as the weapon resource. Give two identical rifles;
seed selected magazine12 and the other7 through existing setup commands, and
join a second owner for isolation. Obtain
actual binding from an ordinary equipped image's genuine native Started record,
then let that setup clip finish and drain setup evidence. Busy has no script or
sequence, so it cannot create a parasitic playback. No guessed generations,
injected result or fake external clock; this is host composition, not AA fidelity.
Use existing `weapon_view` for image state and a fixture readback command that
copies live `p.magazine.rounds` into `package_state` for same-tick ammo inspection.
After ordinary stepping, verify `Notice::Ammo`/`InventoryAmmo` through
`take_private_notices`, including the stowed rifle. No new public identity/ammo
getter or test-only production setter is needed; readback never changes weapons.

1. Fill the real clock outbox with 256 admitted zero-duration body playbacks via
   existing prepare/commit: 512 actual Started/Natural records, no active entry.
   Preserve expected IDs/order without draining the outbox before the attempt.
2. Baseline red: a test-local legacy composition first uses the existing fixture
   exchange command (set_rounds31/set_image_stateBusy), then attempts the real
   weapon clock. Its actual capacity refusal leaves31/Busy. The desired
   unchanged12/Ready assertion must fail at runtime. Record it as the absent
   atomic boundary, not a bug in independent APIs or executed AA source behavior.
3. Green: replace only that test-local attempt with the proposed Session method.
   The same capacity refusal leaves12/Ready, other rifle7, no new Ammo notice,
   unchanged active/revision and exactly the original512 records. Drain and
   verify all original IDs/outcomes/order; do not discard evidence to hide loss.
4. Retry the same request nonce after draining. It succeeds with one actual
   Weapon PlaybackId, next serial after the last fill ID, one Started, loaded31
   and Busy in the same authority tick. This proves refusal burned no identity
   or nonce. The other rifle/owner remains unchanged. Step the real native clock
   to Natural; loaded remains31 and Busy remains Busy. No Finish or second swap.

Retain the green scenario. Add compact variants for bad expected/range/state,
wrong target/stale binding, duplicate nonce after Natural with an otherwise valid
31->0/Ready patch, and occupied target refusal without replacing its playback.
Add holster-after-admission: Weapon custody remains valid and reaches its real
Natural while unmounted; no cancellation solely for selection/mount loss. Existing
clock MAX revision/reservation controls and original native/identity/trajectory
controls remain required; do not add production setters for exhaustion tests.

Run the behavioral baseline red only after a test grant, then targeted Session
controls, sim quick/focused and the retained weapons controls under two Cargo
jobs with target unset. No test/code was added or executed for this proposal.

## Dependencies kept outside this slice

The future package adapter must authorize package/provider/namespace and actual
caller context, stage bounded package state before calling, and have no remaining
fallible publication after host success. This host method is not that transaction
or a general script admission capability. Started observation/async ACK and dual
host/package record delivery remain separate root grants.

Original ToolFire identity must be captured at command/cue creation from the live
Actor/Equipped, forwarded through original command and OpCall, then revalidated.
Typed commands or a `from_image` boolean cannot assert original provenance.
ToggleLight current-context capture, queued old-item rejection and duplicate-item
controls are dependencies, not permission to edit those paths now.

Admission requires current selection/mount. Existing Weapon clock lifetime after
admission uses inventory custody and may outlive that mount; Image lifetime is
different. A future observer must distinguish current-selection applicability
from custody validity. Do not strengthen lifecycle cancellation, remap Pawn15,
drain host records for a package or interpret NativeNatural as source Finish.
AA's exact source completion/fire and dedicated Sleep policy remain separate;
the ordinary private consumer2d977b86 and A1/N1 stay red/open until consumed.
