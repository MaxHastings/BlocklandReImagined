# 2026-10-16 Preserve animation provenance across weapons checkpoints

Root assigned the release-blocking deterministic save/restore regression on exact
candidate 5ec54e60afd7a96a3c45d9d4f2e76b2ce1380608. Reused the clean worker checkout
on codex/v031-animation-provenance-checkpoint; previous853d02f3 remains preserved
on codex/authoritative-animation-clock-impl. Root/gate/v0.3.2 checkouts untouched.
Reserved only weapons runtime/persistence.rs, directly related identity tests and
this note. Read the current platform principles and alpha schema policy.

Baseline command: cargo test -p bri-weapons --test runtime
save_restore_preserves_authoritative_trajectory_and_image_clock --locked --
--include-ignored. The actual converted content test failed, synthetic passed:
one passed/one failed/zero ignored. Uninterrupted producer serials7..12 became
restored1..6. The complete event and subsequent checkpoint equality assertions
remain unchanged. Log: target/animation-provenance-save-baseline.log.

Actor serialization already preserves item custody stamps, actual image mount
stamps, life/body ownership and deferred queue-time bindings. WeaponsSave omitted
the shared animation_serial allocator, so restore reset it to zero. Scanning live
instances cannot recover serials already emitted or belonging to removed items.
The checkpoint now carries and restores that high-water mark explicitly. Alpha
weapon save schema increments3 to4 with no default/migration for the new field.
Earlier weapon checkpoint schemas are rejected under the alpha contract. Ordinary
build saves and their behavior are untouched; weapon content schema is unchanged.

Restore validates live instance and deferred binding generations against the
watermark, rejects duplicate live generations and inconsistent inventory stamp
shape, and preserves the exhausted u64::MAX state without identity reuse.
Unstamped occupied instances are valid only after allocator exhaustion.

Three new checkpoint identity tests first failed on the baseline: deferred event
equality, missing/rewound allocator acceptance and exhaustion identity reuse.
Log: target/animation-provenance-checkpoint-red-corrected.log (zero passed/three
failed/zero ignored,8.56s compile/.38s tests). An initial test-only compile used
Arc<Pack> where restore takes Pack; corrected dereferenced clone before recording
red behavior (target/animation-provenance-checkpoint-red.log).

Commands set CARGO_PROFILE_DEV_DEBUG=0, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=2; CARGO_TARGET_DIR remains unset. No full gate/push, original
content writes or interactive gameplay. Final scoped evidence follows.

The first corrected identity run passed six/failed one: the new removed-instance
fixture compared an undrained actor-removal transient event queue with a restored
world whose events are read at the checkpoint tick boundary. The fixture now
consumes those events with an ordinary step before saving. All complete future
event/save equality assertions remain. Log:
target/animation-provenance-checkpoint-green.log. Corrected target passes seven,
zero failed/ignored (target/animation-provenance-checkpoint-green-corrected.log).
The original unchanged runtime save/restore target now passes synthetic and actual
converted content, two/zero/zero,3.38s compile/.19s tests:
target/animation-provenance-save-green.log.

Final validation on the unchanged code, Cargo two jobs:

- `python tools/check.py quick --crates bri-weapons`: two steps/17s,13 passed,
  zero failed/ignored (target/animation-provenance-quick.log).
- `python tools/check.py focused --crates bri-weapons --depth 1 --tests
  bri-weapons:animation_identity --tests bri-weapons:runtime`: twelve steps/2857s,
  2,695 passed/zero failed/303 ignored across232 summaries; weapons all-target
  Clippy passed. Direct dependent suites addon-import, chaos, client, net,
  package-runtime, sim and weapons-import all passed, as did weapons and both
  explicit targets. Log: target/animation-provenance-focused.log.
- Identity target seven passed/zero failed/ignored in the final tier. Runtime
  target32 passed/zero failed/25 ignored there; the exact content save/restore
  regression was additionally enabled and passed in the targeted two-case run
  above. No ignore flags or equality oracles were weakened.
- Final whitespace diff check passed. Exactly three paths: weapons checkpoint
  implementation, animation identity tests and this progress entry. Immutable
  correctness commit is based directly on5ec54e60. Root owns successor integration,
  full gate/platform/package checks and publication; earlier5ec gate failures
  remain historical evidence, not a successful release check.

No new mechanism or Stage2 work. Previous853d02f3 branch is retained. Keep this
checkout available for concrete root integration regressions until the correction
has been integrated; no finished-worktree target cleanup while root needs it.
