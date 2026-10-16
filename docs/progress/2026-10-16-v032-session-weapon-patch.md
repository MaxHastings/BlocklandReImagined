# 2026-10-16 Session native weapon clock and prepared patch

Root granted only a new inherent method in session/animation.rs, a new
sim/tests/weapon_animation_patch.rs and own documentation, after proposal
30b5d7927e6406b0eefb59e153451db36a40ab98. Accepted weapons helper db12842a
is unchanged. No existing clock function, export/visibility, package, original
ToolFire provenance, observer, client, wire or other production path changed.

## Composition and boundaries

`Session::start_weapon_animation_with_patch(&mut self, owner: OwnerId,
request: AnimationStart, expected_rounds: Option<u32>, rounds: Option<u32>,
image_state: Option<&str>) -> Result<PlaybackId>`.

Adds one 62-line trusted-host method. Requires a matching Weapon inventory slot,
ordinary right-hand0, rate1/tween0, finite nonloop native clip and vacant logical
target. Existing animation_clip validates living peer, current binding/model and
verified native resource before splitting Session fields. Capture authority tick,
borrow animations/weapons separately, prepare the accepted weapon guard, prepare
the real clock, refuse a duplicate receipt, commit clock, consume infallible
patch guard, return actual ID. No whole-Session call or callback while guard
lives, no further Result-producing operation after clock success.

Audited existing AnimationClocks::commit: revision/catalog/start-budget/owner/
session/pose consistency checks all precede its first finish/mutation. It was
not widened or modified. Existing128 occupancy,512 records, revision headroom,
terminal reservations, nonce/receipt/serial rules and NoHold semantics remain.
No new counter, ticket, clock, queue, schema or general transaction abstraction.

Admission requires current selection/mount; existing Weapon lifetime afterwards
uses inventory custody and survives holster. NativeNatural is observational,
never source Finish or a second ammo transfer/refund. Body/Pawn15/retention
activation is absent. Future package/provider authorization, speculative package
state publication, original callback capture and asynchronous observation remain
separate grants. AA2d977b86 remains the strict red ordinary consumer; AA source
review e4402c42 is compatibility evidence only. A1/N1 remain OPEN.

## Actual Session red/green evidence

The new source-free fixture registers a verified native rifle Shape with probe,
reload, zero and loop clips, explicit actual image model/resource/hash binding.
All capacity-fill playbacks use that real weapon resource, avoiding unnecessary
body metadata/roles. Actual binding comes from ordinary equipped image Started;
no guessed generations, injected terminal or fake external clock. Existing
setup commands seed two identical rifles12/7; live player(id).magazine readback,
weapon_view and owner-private ammo projections observe native state. No source
AA assets or production policy are copied or changed.

Baseline test-local attempt used existing independent operations:
fixture legacy command set_rounds31/set_image_stateBusy, followed by actual host
prepare/commit. With512 genuine Started/Natural records already filling the
outbox, real clock capacity refusal left31. Strict unchanged12 assertion failed
at runtime, exit101, 0 passed/1 failed. This demonstrates the missing composition
boundary, not a defect of standalone APIs or executed AA source behavior.
The legacy command/helper was then removed and replaced with the new host method;
the unchanged assertion and later acceptance now pass. No failing/ignored
baseline is shipped. First fixture run separately caught owner-ID-as-player-view
readback; corrected to player(id).magazine before recording the intended red.

Seven retained Session controls pass (0 ignored, 0.06s):

- Real512-record refusal preserves12/Ready, clock active/revision, every original
  record ID/order and no new Ammo notice. Drain and retry SAME nonce: next serial,
  one real Weapon Started,31/Busy at authority tick; second rifle7/other owner
  unchanged. Real Natural later leaves31/Busy with no second transfer.
- Sixteen malformed/options/stale binding/expected/range/state/nonce refusals;
  no native/weapon writes or burned nonce; correct retry accepted.
- Occupied target does not replace/interrupt. Duplicate after Natural cannot
  apply otherwise valid31->0/Ready writes or burn the next serial.
- Holster preserves admitted Weapon custody until its genuine Natural.
- Actual duplicate selection, same-ID remount and ordinary Suicide reject old
  admission binding without clock/image mutation.
- Zero duration gives Started then Natural at one authority tick, patch once,
  one native ammo notice, Busy retained.
- Fill REAL8192 queued ammo events with real zero-duration admitted clocks,
  draining only their actual records. Event-capacity refusal occurs before clock
  mutation; same nonce retries with next serial after ordinary event draining.

## Validation

All own Cargo processes: CARGO_BUILD_JOBS=2, RUST_TEST_THREADS=8,
CARGO_TARGET_DIR unset. Source/tests frozen before the focused tier.

- New Session target:7/0/0.
- Existing native completion15, evaluated phase9, retained pose16, body assets2:
  42/0/0.
- `python tools/check.py quick --crates bri-sim`: format plus406 passed/0 failed/
  4 existing ignored,50s; includes existing private clock revision/MAX controls.
- Retained weapons identity7, prepared guard10 and compile-fail borrow2: pass.
- Original synthetic and converted save/restore trajectory/image-clock cases
  with --include-ignored:2/0/0. Read-only main weapons-pack-009/weapons.json
  SHA256 before/after unchanged:
  7BA8B577A0F290CCA08523237F679393D0F965C1D0916B08AA2B7AAC978E298A.
- `python tools/check.py focused --crates bri-sim`: all8 steps pass,1216s,
  2415 passed / 0 failed / 251 existing ignored across204 summaries. Counts:
  addon-import229, chaos272, client590, net217, sim1104 and stresslab3.
  Warnings-denied all-target sim Clippy passes,33.59s. No failure waived.
- Staged git diff --check passes; staged Rust sources equal tested sources.
  Production diff is solely the new62-line method; db12842a weapons helper and
  all existing clock function bodies remain unchanged. Target unset verified.

Logs: target/session-weapon-patch-{fixture-error,baseline-red,targeted,quick,
native-controls,weapons-controls,borrow-controls,trajectory,focused}.log.
No tests/oracles/ignores changed outside the new target. No full gate, main
merge/push, publication or interactive playtest. Root reviews the immutable
helper before prioritizing actual owning-package AA admission consumption;
this is not another grant to expand low-level mechanisms.
