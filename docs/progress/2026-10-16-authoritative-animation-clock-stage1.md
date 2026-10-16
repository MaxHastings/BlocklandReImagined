# 2026-10-16 Authoritative native animation clock Stage1

Implemented from immutable design b59145b7ad8044a2f0ec090ab9585b7828e93144,
parent sampler b37b280f. Branch codex/authoritative-animation-clock-impl in the
same retained shared-engine-seams worktree. Root granted the eleven proposed
production files, existing constructor/match tests and semantic tests/docs.
Additional explicit grants: package/src/environment.rs hash_bytes using existing
sha2 plus known vectors; weapons/tests/addon_seams.rs exhaustive match; runtime/
sports.rs post-release origin capture and all four actual inventory-removal stamp
clears. No new worktree/subagents/manifests/dependencies/protocol fields.

Stage1 provides immutable verified native catalog, actual producer provenance,
life/body/item/mount epochs, pure prepare/atomic commit, active view and bounded
Started/terminal outbox. No package hooks, source busy Finish, require transaction,
wire replication or client channel15 phase adapter is implemented in this stage.
Those remain Stage2 and necessary for AA acceptance. Existing cosmetic cue fields
and event order are preserved. No game-name heuristics or guessed source timer.

AA PR48/private8046f028 original rifle141 keys/30fps has visual interval140/30 but
original GetAnimDuration141/30; independent pawn clips/rates/tween/channel15 also
need original dispatch/timing evidence. Read its reload-clock consumer note and
source contract. Stage1's full native Animation.duration endpoint does not claim
that source law, alter key spacing or make AA playable. Nonzero tween and other
unimplemented source options remain Unsupported. AA/Source original evidence,
including Source13 threshold/section/loop residuals and AA prone double rotation,
remain with their owners and root.

Commands use CARGO_PROFILE_DEV_DEBUG=0, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4, CARGO_TARGET_DIR unset. No visible game, original installs
unchanged, no gate/rebase/push/main integration by this worker.

- First cargo check -p bri-sim --tests exposed the existing private animation
  helper caller in runtime/sports.rs. Root granted actual actor capture there,
  then requested/approved a batched lifecycle audit. Only sports.rs had additional
  submodule inventory removals: four sites, all stamp clears; no other submodule
  image/emote replacements. Log: target/animation-clock-first-check.log.
- Second cargo check -p bri-sim --tests --locked passed in52.37s after the helper
  update. Log: target/animation-clock-second-check.log.
- Initial enabled native completion target passed four/zero failed/zero ignored,
  2m03s compile/.04s tests. Strict design cases now inspect actual durable host
  state, verified clip identity and lifecycle stamps, not optional cue correlation.
  Natural host outcome does not run the deliberately unregistered package hook
  or clear its busy state. Log: target/animation-clock-native-first.log.
- Expanded Session target passed ten cases, then fifteen after the final
  lifecycle audit. Final corrected run: fifteen passed, zero failed/ignored,
  .05s tests (target/animation-clock-native-corrected.log). Coverage includes
  pure failed admission, stale tickets, receipt eviction, bounded outbox terminal
  reservations, pause/rate/loops, zero-duration order, same-tick births and body
  replacements, atomic catalog rejection, command T versus weapon T+1, deferred
  stale producers, independent pawn/image/inventory weapon lifetimes and
  disconnect/resume invalidation. No package Finish is invented.
- Runtime identity target initially needed Query::can_catch in its test fixture;
  corrected target passed four/zero/zero (18.13s compile, .00s tests;
  target/animation-clock-identity-first.log and -second.log). It checks identical
  item IDs in different slots, temporary actor/image removal, queue-time bindings,
  new drop/pickup custody, equal-ID inventory replacement and worn-image origin.
- Common metadata target first passed two/zero/zero (3m32s compile, .32s tests).
  Adding the ambiguity/budget case exposed noncanonical temporary fixture roots
  against the existing contained-path API. Canonicalized only those fixture
  directories; corrected final target passed three/zero/zero (2.47s compile,
  .29s tests; target/animation-clock-metadata-corrected.log). Dedicated/common
  HostSetup retain the same verified native clip identity with separate sessions;
  damaged bytes/wrong stock hash are rejected; optional conflicting/excess image
  metadata preserves stock hosting rather than breaking the existing host.
- Final correctness audit reserves revision space for admitted terminals and
  validates stale producer bindings before root-stop processing. Inventory weapon
  clocks retain custody through holstering while pawn/image clocks cancel.
  Native catalog admission shares its resource count/byte budget with the loader.
  Original repeat, promotion, Finish, tween and dedicated duration laws remain
  unimplemented where evidence has not established their acceptance.

Root relayed the v0.3.1 release freeze during this stage: finish only the granted
Stage1 correctness/lifecycle/validation and immutable handoff. Stage2, fitting,
Source and new Halo mechanisms are queued for v0.3.2, not automatic follow-up.
AA/GMod are parked; original remaining fidelity is no longer a v0.3.1 blocker.
Root subsequently relayed Maxwell's explicit authorization to publish v0.3.1
automatically once ready; no final user playtest/approval wait remains. Root owns
integrated/platform/package checks and publication. This changes no worker scope,
test requirement, interactive testing boundary or private asset restriction.

The granted session/packages.rs needs no edit: install_packages already forbids
replacement with live players/departed state, and common HostSetup installs the
verified animation catalog afterward. No additional package policy was added.

- First quick run exposed a client constructor moved-path borrow (159s,
  target/animation-clock-quick.log). Metadata construction now runs inside the
  existing spawn_blocking content loader and returns beside weapon_pack; only
  the avatar metadata clone enters the worker, with no adapter behavior change.
- Corrected quick: `python tools/check.py quick --crates
  bri-package,bri-weapons,bri-sim,bri-net,bri-client`, ten steps/227s;
  1,144 passed, zero failed, 83 ignored across five unit summaries. Log:
  target/animation-clock-quick-corrected.log.
- Focused ran with the same five crates, --depth 0, and explicit targets
  bri-sim:authoritative_animation_completion, bri-weapons:animation_identity,
  bri-weapons:addon_seams, bri-net:animation_metadata, bri-net:host_setup and
  bri-client:trench_dig_flow. First run stopped on collapsible_if in optional
  ambiguity handling (34s, target/animation-clock-focused.log); equivalent
  collapsed condition fixed, with no behavioral change. Corrected final run:
  21 steps/895s, five all-target Clippy checks, 2,148 passed/zero failed/248
  ignored across 154 summaries. Log: target/animation-clock-focused-corrected.log.
  Exact explicit targets are Session15, runtime identity4,
  addon seams13, metadata3, host setup5 and trench flow1, all zero failed/ignored.
  Root owns broader integrated/platform checks. No full gate run by this worker.

Final implementation paths (21, including this entry):

- crates/sim/src/session/animation.rs; session.rs; session/combat.rs;
  session/inventory.rs; session/weapons.rs;
  sim/tests/authoritative_animation_completion.rs.
- crates/weapons/src/runtime.rs; runtime/sports.rs;
  weapons/tests/animation_identity.rs; weapons/tests/addon_seams.rs.
- crates/net/src/content_identity.rs; dedicated.rs; host_setup.rs;
  net/tests/animation_metadata.rs; net/tests/host_setup.rs.
- crates/client/src/app/session.rs; client/tests/trench_dig_flow.rs.
- crates/package/src/environment.rs.
- docs/architecture/authoritative-animation-completion.md;
  docs/architecture/seams.md; this progress entry.

Working tree diff whitespace check passed. Immutable handoff uses this design
checkpoint as parent; no main rebase/gate/push, target deletion or original content
changes. Root reported a separate sampler test oracle correction against its
integrated candidate; that root-owned test is untouched here. Do not infer a full
release gate or original-game fidelity acceptance from these scoped results.
