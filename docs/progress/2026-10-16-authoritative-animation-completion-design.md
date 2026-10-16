# 2026-10-16 Authoritative animation completion design

Root assigned this design/red-reproduction atom from clean sampler b37b280f.
Branch codex/authoritative-animation-completion-design, same reused worktree,
no production edits/manifests/dependencies/subagents. Read existing mounted pose
plan, actual Session/runtime/package producers and AA PR38/private3f593e07
handoff plus its magazine policy/source evidence. Original installations unchanged;
all tests native/headless, no interactive session.

Existing play_thread and weapon Animation events emit cosmetic cues only. Native
clip metadata is available but not retained as clock input. Actual runtime item
and Equipped mount incarnation stamps are absent. Raising the package slot cap
alone or taking render/audio completion cannot produce truthful results. A narrow
Session clock stage can precede full mounted fitting, with explicit admission,
generation ownership and package delivery. Exact API/proposed footprint and
source/lifecycle/replication constraints are in the new architecture note.

The sim fixture installs invented shared native body clips and a real server Rhai
package, then exercises actual play_thread and ordinary weapon state entry.
It characterizes finite native endpoint / no busy completion after600 ticks and
indistinguishable animation payloads from different inventory slots. Strict ignored
requirements propose admitted correlation fields for root review; they are not
an implemented API or a green acceptance claim. Do not integrate this deliberately
red branch alone; eventual production correction must enable the cases and add
real Started/Natural/Interrupted/Cancelled and lifecycle/transport tests.

Commands use CARGO_PROFILE_DEV_DEBUG=0, CARGO_INCREMENTAL=0,
CARGO_BUILD_JOBS=4, CARGO_TARGET_DIR unset. Test results/failures follow.

- Initial fixture compile exposed incorrect tuple syntax for the actual
  WeaponTrigger struct variant and borrowed values where package_value returns
  owned JSON. Corrected only the test. Log:
  target/authoritative-animation-completion-baseline.log.
- Actual first executed --include-ignored: two controls passed, two strict
  provenance requirements failed, zero ignored; 7.52s compile/.03s tests. The
  failures print the actual emitted body/thread2 and image/thread0 payloads with
  no playback correlation. Log: target/authoritative-animation-completion-second.log.
- Added an explicit actual selected-body assertion; its first compile used
  Archetype.model instead of Archetype.look.model. That exit101 is a compile
  failure, not additional red evidence. The sequential wrapper checked only
  exit101 and proceeded to quick; final evidence must check actual executed test
  result summaries, not an exit code alone. Log:
  target/authoritative-animation-completion-red-compile.log. Quick still passed
  403 tests/four ignored in69s; focused correctly caught the same fixture field
  error at Clippy after34s. Preserved first-tier logs; corrected the actual field
  before restarting. The diagnostic wrapper now requires the exact executed
  two-passed/two-failed summary as well as exit101.
- Read actual Session tick order: package work sees published T, physics advances
  authority to T+1, weapons reads T+1. Refined the proposal to advance clocks once
  after sync_seated/before weapons at T+1, deliver queued outcomes next package
  phase at published T+1, and preserve real admission ticks. No production order
  changes or pre-fire/current-fire retroactive sampling are proposed.
- Corrected selected-body --include-ignored run executed four cases: two passed,
  two strict provenance failures, zero ignored; 7.94s compile/.02s tests. This is
  actual assertion evidence, not just exit101. Log:
  target/authoritative-animation-completion-red.log.
- Corrected `python tools/check.py quick --crates bri-sim`: two steps in6s,
  403 passed, zero failed, four ignored. Log:
  target/authoritative-animation-completion-quick.log.
- Corrected `python tools/check.py focused --crates bri-sim --depth 0
  --tests bri-sim:authoritative_animation_completion`: four steps in253s,
  82 result summaries, 1,056 passed, zero failed, 130 ignored. All-target Clippy
  -D warnings passed. Full sim suites precede the explicit repeated target;
  its two diagnostic requirements stay explicitly ignored in this default tier,
  and fail in the separately executed strict run above. No full gate or claim
  that the missing clock is implemented. Log:
  target/authoritative-animation-completion-focused.log.
- Root approved clock-first direction with durable active-view authority and
  requested exact Stage1 metadata/admission/epoch footprint before any production
  edits. New architecture section lists eleven production paths, two mechanical
  SessionContent test constructors and proposed semantic test paths. Package
  hooks/rollback/replication/client channel15 remain Stage2 and required for AA.
  Raw cue correlation tests are provenance diagnostics, not an obligation to add
  optional cue fields or an acceptance shortcut using tick ids; Stage1 correction
  must adapt them to actual approved typed active/terminal state.
- git diff --check passed. Only four reserved design/test/doc paths changed;
  immutable checkpoint and exact Stage1 proposal go to root before production.
