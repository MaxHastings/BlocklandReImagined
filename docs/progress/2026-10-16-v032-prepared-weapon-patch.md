# 2026-10-16 v0.3.2 prepared exact-instance weapon patch

Root granted only crates/weapons/src/runtime.rs, a new
crates/weapons/tests/prepared_weapon_patch.rs and own documentation, directly
after docs444ae4bc. Read root's approved v0.3.2 readiness48ffe16c: Chief/AR/
Warthog required, AA A1 portability consumer still OPEN, shotgun independent.
This prerequisite does not close ordinary package admission, reload completion
or any release row. Existing authoritative-animation seam ownership remains
with root and this isolated v0.3.2 worker; no second engine owner or allocator.

## API and invariants

`WeaponsWorld::prepare_weapon_patch(&mut self, id: ActorId,
binding: &AnimationBinding, expected_rounds: Option<u32>, rounds: Option<u32>,
image_state: Option<&str>) -> Result<PreparedWeaponPatch<'_>>`.
`PreparedWeaponPatch::commit(self) -> ()`; drop aborts.

Preparation validates positive actual owner life/body, selected item custody and
ordinary right-hand mount identities. It refuses missing/foreign/stale bindings,
wrong expected loaded rounds, out-of-range desired rounds and unknown image
states. Loaded-round writes require a real stored magazine and its exact
selected-slot key; reserve-counted/unlimited supply cannot masquerade as loaded
rounds. Optional state-only or expected-only patches remain supported.

Guard holds an exclusive borrow plus a prepared actor snapshot and reserved
event-vector capacity. It is neither Clone nor Copy. All cloning, lookups,
state/flag calculation and fallible reservation happen before returning it.
Commit publishes that snapshot and, for a rounds write, one reserved Ammo
notice. It performs no lookup/fallible call or image-entry callback. Ordinary
state entry remains deferred until the existing weapons advance. There is no
serial, clock, save-schema change or changed behavior for existing callers.
Consuming one guard prevents reapplying that preparation; it does not deduplicate
new requests. Future clock nonce/receipt checks must reject action replay before
another patch is committed. Expected-round checks alone are not an action ID.

The actor snapshot preserves the other inventory instances, reserve, pending
cues, triggers and identities. Borrowing prevents intervening mutations and
pack changes while the guard is usable. Drop after an external refusal leaves
actor state and pending events unchanged; vector capacity alone may have been
reserved. OOM/panic process recovery is not a transactional guarantee.

This primitive checks the host's recorded life/body stamp; the future Session
adapter must still enforce actual living-player eligibility. Current
Session::commit_animation_start borrows the whole Session and cannot be called
while its weapons guard remains borrowed. A future reviewed adapter must split
disjoint clock/weapons fields, finish Session-level validation before acquiring
the guard, commit the independently borrowed clock, then consume this guard.
The primitive was not exercised through Session admission or source completion;
the real weapons-state guard controls do not establish that integration.

Read AA compatibility review e4402c420d0ead7a63ef31fd7916aa2135bad7df,
aa25/notes/2026-10-16-m16-admission-observation-review.md. It accepts the
bounded normal immediate exchange/state-only observation proposal, not executed
source completion. A1/N1 remain red/open. Important clarification for the next
adapter: Weapon playback custody can outlive selection/mount. This helper's
current-selection/mount checks qualify admission only; they do not cancel a
clock or redefine its identity/lifetime. Future observer current-selection
applicability must be distinct from custody validity. No clock behavior changed.

## Validation

All Cargo processes use CARGO_BUILD_JOBS=2 and RUST_TEST_THREADS=8;
CARGO_TARGET_DIR remains unset. No original content or source is written.

- Final prepared_weapon_patch target: 10 passed, 0 failed/ignored. Controls cover
  bad expected/range/state with preexisting pending events, fourteen incomplete/
  forged/foreign bindings, stale life/body/selection/redraw/custody/removal,
  correct duplicate-rifle instance, other actor/reserve/serial preservation,
  abort, exact legacy flag/state semantics and deferred single entry, optional
  writes, event capacity8192, unsupported loaded-round supply, and restored zero
  life/body epochs even when the supplied binding matches.
- Doc tests: 2 compile-fail controls pass, proving the guard cannot be committed
  twice or survive an intervening mutable world operation.
- Original animation_identity target: 7 passed, 0 failed/ignored.
- Original authoritative save/restore trajectory and image-clock case with
  --include-ignored: synthetic plus converted content, 2 passed, 0 failed.
  BRI_WEAPONS_PACK points read-only to main content/weapons-pack-009/weapons.json;
  SHA256 before/after identical:
  7BA8B577A0F290CCA08523237F679393D0F965C1D0916B08AA2B7AAC978E298A.
- Final `python tools/check.py quick --crates bri-weapons`: format and 13 unit
  tests pass, 1s.
- Final `python tools/check.py focused --crates bri-weapons`: all 11 steps pass
  in 1090s, 2710 passed / 0 failed / 278 existing ignored, across 242 summaries.
  Warnings-denied all-target weapons Clippy passes on final source in 4.20s.
  Per-crate passed counts: addon-import229, chaos272, client590, net217,
  package-runtime100, sim1097, stresslab3, weapons191 and weapons-import11.
  No tests/oracles/ignores changed outside the new target, and no failure waived.

First focused run passed all11 steps in1955s: 2710 passed, 0 failed,
278 existing ignored,242 summaries across addon-import229, chaos272,
client590, net217, package-runtime100, sim1097, stresslab3, weapons191 and
weapons-import11. This run began before the zero-epoch hardening below, so it
is preserved as focused-initial.log rather than substituted for final-source
validation. The complete final rerun above has final code/tests from its start.

While the first focused run was in its network stage, read-only review found
that existing weapons restore leaves host owner-epoch validation to the host.
Hardened this new helper to refuse actual zero life/body epochs even if a
supplied binding matches, and added a tenth targeted control using such a
restored actor. No restore/save path changed. The running build was not
interrupted; final-source targeted/doc/quick/trajectory checks and the complete
focused tier were rerun before commit, rather than presenting the earlier
predicate's lint as final evidence.

First targeted compile caught two fixture errors (nonexistent ReserveOnly
variant and missing mut on FnMut); corrected to existing Counted and mut before
the first green run. Removed constant external-clock stand-ins before final
validation; tests assert actual weapon state/events and do not claim Session
clock composition. No runtime assertion, ignore or acceptance oracle was weakened.
Final logs: target/prepared-weapon-patch-{targeted,doc,trajectory,quick,focused}.log;
targeted contains both the 10 new guard and 7 original identity tests. Earlier
identity/initial focused logs are retained. Staged git diff --check passes;
only the two granted Rust paths and this entry changed from docs444ae4bc.

No package/sim/provenance/observer/client/wire production paths changed, no
general callbacks, full gate, main merge/push, publication or interactive test.
After immutable handoff, root reviews the primitive; the next dependent bridge
still needs exact paths and AA validation through the same ordinary consumer.
