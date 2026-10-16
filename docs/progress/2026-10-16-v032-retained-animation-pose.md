# 2026-10-16 trusted-host native final retention (D2b-1)

Root granted only D2b-1 from immutable proposal
`3d44bfb7264178b17069d695cb4f059996d3b919`: animation.rs, session.rs exports,
BodyPoseInputs::info visibility only, new retained_animation_pose.rs and own docs.
The lane remains isolated v0.3.2; no main/v0.3.1 integration or D2b-2 producer
activation, weapons changes, package/wire/hook/counter, origin, vehicle or
motor/client extraction is authorized or implemented.

Pre-production behavior was exercised through actual Session admission/steps
with invented validated native clips and ordinary package admission. The 15
existing authoritative clock controls, D2a's nine controls and default Natural
removal control passed. The future pose adapter's final-sampling expectation
failed with None versus Some(0.25), proving the behavioral gap before any new
API existed (`target/d2b1-baseline-corrected.log`). This is separate from a
missing-method compilation failure. The initial fixture incorrectly put script
and behaviour in a shared package; ordinary admission rejected it. Splitting
server rules from shared body definitions fixed the fixture without weakening
validation (`target/d2b1-baseline.log`).

Implementation adds a separate trusted-host retention option, pose view and
PlaybackId-specific clear. AnimationStart's serialized schema, default NoHold
producers, D2a active snapshots and legacy anchors remain unchanged. Opt-in
requires Pawn2, actual D1 body inputs and a clip whose resource/digest belongs
to those inputs. Applicability retains the semantic full body receipt and all
four actual ordinary image identities, including worn2/3, excluding separate
emote3. Staged catalog checks use the existing D1 info resolver via its sole
visibility change. Equivalent semantic receipts can survive an Arc replacement.

Active/held target keys share limit128; there is no eviction, TTL or separate
pose counter. Only active clocks reserve terminal records under the independent
512-record bound. Held entries store native final phase and immutable receipts
at the existing clock owner. Natural emits once; root, clear, replacement and
invalid applicability remove a hold without a second terminal. Queries filter
current applicability read-only; they neither reclaim capacity nor consume records.

Revision admission additionally preserves `revision + K + P <= u64::MAX`,
where K is active-plus-held occupancy and P is opted-in active count. Each
occupied key reserves its eventual clear; P additionally reserves Natural-to-held
transfer. Admission preflights three extra increments against this derived
reserve, rate/catalog changes one. Cancellation/clear/owner cleanup consume the
existing reserve without a new fallible step. All increments are checked; no
second revision or identity counter. This covers future completion plus cleanup,
rather than checking only the immediate start.

Three private-clock controls passed, with no counter test hook: near-MAX
Natural/clear, mixed active/held body/remove cleanup plus catalog headroom, and
the exact occupied/identity validator used by ordinary slot reads, including
emote3 separation (`cargo test -p bri-sim --lib retention_tests --test retained_animation_pose`,
`target/d2b1-expanded-unit.log`). The filter ran three unit controls; the then13
integration cases were compiled but filtered, not counted as executed. Reaching
an occupied mount with no generation via public Session controls would require
exhausting the runtime's private u64 serial; that negative branch is explicitly
a validator unit control, alongside actual Session positive four-slot coverage.

Final targeted command:
`cargo test -p bri-sim --test retained_animation_pose --test authoritative_animation_completion --test evaluated_animation_phase --test body_pose_assets`
passed 41 controls: retention15, existing clock15, D2a9 and D1 body-input2
(`target/d2b1-targeted-corrected.log`). Covers actual four-slot mounted state,
same-ID worn and hand remount, emote-only replacement, pending equipment,
duplicate selection, take/drop/contact-pickup transfer/loadout replacement,
death/body/tune/birth/disconnect, failed/stale preparation, root/zero/loop/rate,
receipt eviction, full128 mixed occupancy and independent512 record pressure.
The earlier targeted run passed13 retention controls and failed two fixtures:
receiver spawn was obstructed, and a dead player could not issue a package birth
command. Receiver placement now uses the existing pickup-test offset; death
uses the actual public respawn deadline and Respawn control. No production
rule was weakened (`target/d2b1-targeted.log`). Initial two-control green evidence
is separately in `target/d2b1-first-green.log`.

`python tools/check.py quick --crates bri-sim` passed formatting and 406 unit
tests, zero failures, four existing ignored, in 48s (`target/d2b1-quick.log`).
`python tools/check.py focused --crates bri-sim` passed all eight steps in
1972s, including sim all-target Clippy with warnings denied and the standard
dependent closure (`target/d2b1-focused.log`). Across203 test summaries:

| Crate | Passed | Failed | Existing ignored |
| --- | ---: | ---: | ---: |
| bri-addon-import | 229 | 0 | 3 |
| bri-chaos | 272 | 0 | 26 |
| bri-client (unit) | 590 | 0 | 76 |
| bri-net | 217 | 0 | 20 |
| bri-sim | 1097 | 0 | 126 |
| bri-stresslab | 3 | 0 | 0 |
| Total | 2408 | 0 | 251 |

The sim run executed all16 retention tests and the three private controls, plus
the existing D2a/clock/D1 controls. The standard tier's existing ignores remain
unchanged. No test was waived. Runtime checks used process-local
CARGO_BUILD_JOBS=2 and RUST_TEST_THREADS=8; CARGO_TARGET_DIR was confirmed unset.
No build was interrupted and no full integrated gate was run in this isolated
lane. Original Halo/AA source behavior and actual native producer opt-in remain
separate requirements.

After the focused tier's initial Clippy passed, added one integration control
in the granted test file only: a real queued weapon root with either stale item
custody or stale image generation must not clear a newer unbound hold; a current
free root must still clear it. Production source stayed frozen. The focused sim
phase executed the final16 retention controls. After focused completed,
`cargo fmt --package bri-sim --check` and
`cargo clippy -p bri-sim --all-targets --locked -- -D warnings` both passed
for that final source (`target/d2b1-final-lint.log`). No
dependent production suite needs a duplicate run for a test-only fixture edit.

`git diff --check` and `git diff --cached --check` passed. The staged scope was
exactly the three granted production paths, new integration test and four own
documentation files. No manifests, lockfile, weapons or renderer files
changed. Keep this handoff separate from the next safe-input grant.

Root subsequently froze this candidate pending its declared tiers. Two items
are queued with no expanded grant: AA's source-backed combined blend/alpha-test/
depth-write requirement, and a future exact safe-input sync containing reviewed
weapons checkpoint9f233b3f. Root identified an older schema3 allocator-restore
bug still present in this v0.3.2 input. Do not merge that sync during validation
or claim restored allocator identity correctness from these normal-runtime
controls. Root will grant the exact input after this clean immutable handoff;
renderer/content and weapons production paths remain untouched.
