# 2026-10-16 Owning-package weapon admission and observation bridge

Root granted twelve named production paths plus the three new test targets on
accepted host composition 6d6e3aa183d61e9edd1edcbc534ee5ca70f40857. This isolated
branch does not merge/push main, publish a release, touch the drop loop, or grant
AA execution on an uncommitted engine copy. Maxwell's bugfix-first v0.3.2 release
does not wait for this bridge or a completed AA/showcase consumer.

## API and atomic boundary

Server behaviour opts in with `on_weapon_animation: true` and must declare
`fn on_weapon_animation(player, info)` with arity two. The player capability adds
`start_weapon_animation(player, resource, sequence, nonce, expected_rounds,
rounds, image_state) -> queued_operation_handle`. Optional patch fields use `()`.
The operation is implicitly required. The handle is not a native playback token.
Nonce must fit a positive Rhai integer; body resource/sequence/state and rounds
are bounded. Rate one, tween zero, weapon slot and namespace are server resolved.

Only an original server-captured ordinary image command may admit. ToolFire
captures the actual owner/item/image incarnation while Actor/Equipped can be
detached; ToggleLight captures at its trusted image command source. Typed and
other unqualified command paths cannot acquire authority from `from_image`.
Queued stale callbacks cannot adopt a replacement mount with the same content ID.
The requesting package must have the observer and compiled script, and its own
or transitive installed dependency's body resource must match the actual native
weapon resource and verified bytes digest. Actual model/binding/life checks and
the accepted host composition remain authoritative.

The call permits exactly one native admission operation and bounded speculative
namespace state. It refuses nested package parts, entity variable writes, emitted
events and other operations. All state authorization/budgets and checked revision
preparation precede native success. Only after the existing clock plus prepared
weapon patch succeeds does the code move the owned namespace into the store.
Refusal keeps namespace, rounds, image state, nonce and native serial unchanged.
No callback or Result-producing publication follows native success.

## Observation and lifetime

The existing bounded 512-record outbox now carries host-pending and optional
owning-package-pending delivery flags on the same payload. An active route is
bounded by existing active clock occupancy; no new counter, second durable queue,
or retry queue exists. Host drain cannot steal package delivery or vice versa.
At step end after reconciliation, the pass-entry package set is marked attempted
before any hook. Work share exhaustion, hook errors or forbidden mutations do
not retry, refund, unlock or alter world state. Hook state is speculative and
uses the same state validation/budgets. Changed live script reload abandons its
old package routes without cancelling clocks or removing unread host evidence.

`info` contains `token` and `binding` as canonical JSON strings preserving all
actual native u64 fields; `nonce`, `target` (weapon), `slot`, actual `resource`
and `sequence`, `outcome` (started/native_natural/interrupted/cancelled), `phase`,
`reason`, decimal-string `authority_tick` and `delivery_tick`, plus booleans
`custody_valid`, `current_selection_applicable`, `current_action_applicable`.
Current action requires live matching owner epochs and the namespace watermark;
custody and current selection remain distinct. Weapon clock lifetime retains
inventory custody independent of the current selection/mount. NativeNatural is
observation, never source Finish or a second magazine transfer. This does not
close AA source completion, A1/N1, Pawn15 policy, or the ordinary AA consumer.

## Controls and corrections

New runtime controls check implicit requirement, hook declaration/arity,
capability and bounds. Weapons controls capture detached callback provenance,
then change duplicate selection/life or remount the same content ID before
consumption. Sixteen Session controls cover ordinary immediate exchange and
actual identity, typed/foreign-owner refusal, provider/dependency/hash/model
checks, real 512-record refusal and rollback/retry serial, both drain orders,
holster custody, hook failure/forbidden world mutation/recursive admission,
queued stale mount, changed-script reload, zero duration/tick payloads, unread
host backpressure, mixed operations/nesting/events/entity writes, death and
duplicate package nonce. All fixtures use authored source-free shapes/packages.

Corrections during implementation: explicit Rhai Fallible return type resolved
closure inference; an external operation type cannot have a sim inherent impl,
so request validation is a free function; an initially misplaced test attribute
inside a Rhai raw string was caught at zero collected tests and moved outside;
fixture mount_image/diagnostic access used existing actual APIs. Catalog already
refuses model mismatch, which the control preserves. Backpressure input is paced
at six steps per command to respect existing action limits. The last fixture
expansion mixed server script and shared model providers; it was split into
proper server observer/policy and shared resource packages without changing
production side rules. Formatting-only unrelated files were restored.

## Validation

Before the final formatting/source freeze, `cargo check -p bri-sim --locked`
passed; runtime 2/0, provenance 2/0 and Session 16/0 targets passed. Required quick tier passed all six steps in 89s: formatting and 456 unit tests,
zero failed, four existing ignored. First normal focused attempt stopped after
1268s at the existing script_docs coverage test because the new function was
missing from docs/modding. All three all-target clippy steps and complete
addon-import, chaos, client and net suites passed before that failure. Added
own new docs/modding/weapon-animation.md with the full bounded API and limits;
no existing shared guide or test was weakened. Final rerun receipts follow below. Every Cargo process uses
CARGO_BUILD_JOBS=2, RUST_TEST_THREADS=8 and leaves CARGO_TARGET_DIR unset. No
visible game or interactive input was used; original installations remain read-only.

Next: root reviews the immutable validated bridge slice and decides integration;
AA production consumer execution requires root's exact-pin authorization. Root
owns the combined gate, packaging and publication.

## Final frozen-source receipt

- `python tools/check.py quick --crates bri-package-runtime,bri-weapons,bri-sim`:
  PASS, 6 steps/89s; 456 passed, zero failed, four existing ignored.
- `cargo test -p bri-package-runtime --test script_docs --locked`: PASS 1/0
  after adding the own modding guide page.
- `python tools/check.py focused --crates bri-package-runtime,bri-weapons,bri-sim`:
  final full normal rerun PASS, 15 steps/1553s, 246 successful test summaries;
  2737 passed, zero failed, 278 existing ignored. All-target clippy with
  `-D warnings` passed for all three changed crates. Completed crate steps:
  addon-import, chaos, client units, net, package-runtime, sim, stresslab,
  weapons and weapons-import; no depth restriction, new ignore or waiver.
  Final-source new API target 2/0, original ToolFire provenance 2/0, package
  Session bridge 16/0; accepted host patch 7/0 and all retained/native phase
  controls remain green in the same run. The two compile-fail borrow controls
  also pass. Logs: target/package-weapon-bridge-{quick,focused-final}.log.
- With process-local `BRI_WEAPONS_PACK` pointing at main's read-only generated
  content/weapons-pack-009/weapons.json, `cargo test -p bri-weapons --test runtime
  --locked save_restore_preserves_authoritative_trajectory_and_image_clock --
  --include-ignored`: PASS synthetic and v20 2/0, 55 filtered. Pack SHA256
  before/after remains
  7BA8B577A0F290CCA08523237F679393D0F965C1D0916B08AA2B7AAC978E298A.
  Log: target/package-weapon-bridge-trajectory.log.

Only own docs changed after the source freeze; the first focused documentation
failure and full rerun are retained above. Root's later Arrival/loading typing
ownership is disjoint: Session.rs changes only ToggleLight capture/forward and
one end-step observer call. No Arrival region, drop loop, manifest/lockfile,
protocol, client, renderer or source content changed. The integrated gate and
publication remain root-owned; no AA consumer/source completion claim is made.
