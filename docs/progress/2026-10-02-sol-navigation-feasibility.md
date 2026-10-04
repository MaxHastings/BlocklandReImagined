# v0.2.1 stability and integration evidence

This audit describes the combined playtest worktree, based on published v0.2.0
`28b3e3248`. It does not revise v0.2.0 or certify interactive acceptance.
Maxwell performs all interactive playtests. Evidence below uses headless tests,
CPU geometry and bounded extraction of his supplied video.

## Report inventory

| Report | State | Evidence and remaining limit |
|---|---|---|
| Bedroom mixed Zombie/Blockhead firefight crashes while changing spawn loadout | **OPEN: cause unknown** | Actual Windows report is a main-thread `f32::clamp` panic with both bounds NaN. Its 33 unknown frames have no addresses. The real-content headless reproduction passes, which does not exercise the client, renderer, audio or Windows backend. No NaN guard is presented as a fix. |
| Crash report contains only unknown frames | Diagnostics improved; Windows/Linux execution pending | Independent native frame capture retains IP, loaded module, base and offset. Mac debug and stripped release child-process reports satisfy address/offset invariants; the stripped release symbolic section degenerates to `__mh_execute_header`, while its 15 native frames remain usable. Matching build symbols are still required. |
| New/classic Duplicator stops selecting after map changes | Fixed in headless lifecycle | `Session::adopt` initializes package player defaults through `packages_joined`. Classic short-click selection before/after adoption passes; New Duplicator passes with the optional actual converted content. These tests do not certify every Duplicator feature. |
| Copied invisible/noncolliding brick has no placement preview | Fixed in CPU geometry | Only render copies are revealed; authored blueprint visibility/collision are retained. Regression requires nonempty preview vertices. Temp-brick styling already supplies visible alpha; no offscreen pixel check is claimed here. |
| Enabling Shark still displays a Blockhead | Body reload repair verified; behavioral parity partial | Prepared reloads now load and install `AvatarAssets` and clear old meshes. Synthetic enable/disable regression passes; a separate real converted Shark CPU test verifies original model, sequence and pixel identity and finite geometry at three scales. These are complementary tests, not an actual interactive reload. |
| Host Admin remains waiting for authority | Fixed in headless request lifecycle | Maps replies now undergo the same stale-state check as other lists. Ack alone does not remove the 45-second request deadline. Missing/stale list replies clear busy state on timeout; a correlated list after ack completes without a false timeout. |
| Portal third-person boom is stopped by its backing wall / recrosses for one frame | Fixed in CPU walking/observer/driver geometry | Walking mirror clips live portal geometry, keeps destination obstacles/closed panes, and nudges the carried boom past a shared doorway plane. Driver rays now use the same geometry near openings. Five CPU crossing regressions pass; interactive/video equivalence remains Maxwell's acceptance. |

## Firefight investigation

Input: `/Users/maxhastings/Downloads/crash-20261003-031241.txt`, from Windows
v0.2.0 on NVIDIA RTX 4070 SUPER/DX12. The report records
`min > max, or either was NaN. min = NaN, max = NaN`, main thread, Rust
`f32.rs:1432:9`. It cannot recover the calling function from the existing file.
The Gun-to-Gravity-Gun spawn edit is a temporal observation, not proven cause;
the user did not fire or grab with the Gravity Gun.

`crates/chaos/tests/bot_firefight.rs` builds a real Bedroom session with eight
Blockheads and eight Zombies. It runs 45 simulated seconds with an unchanged
Gun loadout and 45 with a change to Gravity Gun after eight seconds. All sixteen
bots must spawn and at least one real projectile must fire; MessagePack scans
require finite snapshots and motion states every six ticks. The cleaned test
passed in 46.57 seconds, observing 926 and 198 distinct projectiles respectively.

This narrows the evidence to a passing authoritative simulation case. Missing
reproduction inputs include the original save/build, exact enabled package set
and settings, player positions/actions, and Windows client frame state. A client
or renderer failure remains possible. Candidate scalar bounds include scaled
actor/vehicle bodies, authored look/tether limits and audio descriptors, but none
has a traced causal path. `glam::Vec3::clamp` has a different assertion and is not
itself evidence for the supplied scalar panic. Existing cosmetic casing
zero-normal handling predates v0.2.0; it is not a new fix for this report.

## Native report implementation

Rust's [Backtrace Debug implementation](https://doc.rust-lang.org/stable/src/std/backtrace.rs.html)
formats resolved symbols rather than exposing its stored raw IPs. Switching to
alternate Debug alone does not repair a symbol-free Windows report.

- Windows: `RtlCaptureStackBackTrace`, `VirtualQuery` image allocation bases and
  `GetModuleFileNameW`, with the existing windows-sys dependency and direct
  kernel32 declaration for the module filename API.
- macOS / Linux release target: `backtrace` and `dladdr` image mappings. Linux
  uses libdl. No new crate dependency or manifest/lockfile change.
- A frame without an image mapping still writes its raw IP. Native IPs are return
  addresses, so a symbolizer may need the preceding instruction for the callsite.
- The report retains the normal Rust backtrace, executable path, message,
  location, thread and log tail. Capture is best effort, bounded to 128 frames.

Capture tests parse the actual report, require at least three nonzero mapped
frames, check `base + offset == ip`, and require the test executable's module.
They pass on Apple silicon, including a copied release executable stripped with
`/usr/bin/strip`. Windows execution against an executable without its adjacent
PDB and Linux execution remain required platform evidence; this Mac has only
the Apple silicon Rust target installed. The matching PDB/dSYM/ELF symbols must
be retained by release packaging/CI.

Local review artifacts (ignored, not shipped source content):
`artifacts/sol-stability/capture-release-stripped-test.log`,
`artifacts/sol-stability/release-stripped-panic/crash-20261003-040938.txt`, and
`artifacts/sol-stability/portal-video-contact.png`.

## Driver camera integration repair

The vehicle driver callback previously used `portal_view::ray` with an ordinary
`building.solid_segment` hit. A wall directly behind the opening won that query
before the ray could enter the portal. The production-path regression fails on
that route: source camera `[23.25, 1.575, -4.21]` instead of its expected carried
`[107.25, 1.575, -40.25]` (see `vehicle-camera-negative.log`).

`CollisionMirror::portal_camera_hit` now returns the clipped sphere cast's hit
distance and actual surface normal. The existing player wrapper still applies
its original 0.02-unit margin. Drivers retain their authored normal-dependent
back-off and portal carry; the shared `camera_segment_near_portal` predicate
selects this geometry for both camera paths only near an opening or closed pane.
Ordinary segments retain their indexed query. The driver regression checks the
backing-wall cut, destination wall/normal, closed partner and exact ordinary hit;
ten existing vehicle-camera tests also pass, including authored wall easing.

The existing ignored real-content vehicle-eye test was attempted with
`BRI_CONTENT` and failed before loading a fixture: its implementation hardcodes
this worktree's absent `content/` directory and does not consult that variable.
This is a harness-input limitation, not evidence of a runtime camera failure.
Root subsequently supplied the normal path via a read-only symlink to main
content. Rerunning the exact real-content vehicle-eye test then passed (1 test,
0.61 seconds), covering every authored seat's posed eye node. No test-source
change was needed.

## Body and NPC parity boundaries

Actual converted package inventory contains three bot kinds: Blockhead Bot,
Zombie and Shark. Shark's model is `bot_shark:asset/shark.dts`, mapped to
`assets/models/49f7b88fc656e89ab700a01e.shape.json`, SHA-256
`7378f3b2ca1180aa2a4d51847bbfce13d41db67c51e711c2e8226d410d85d97f`.
It has twelve nodes, six objects, a looping twelve-frame `swim` sequence and
`biteFix`. The real body test loads the actual base avatar role, enables Shark,
poses 1698 vertices/indices, and compares `blank.png`/`black75.png` material
pixels with their actual converted PNGs. At scale one, root-pose extent is
`[3.0759225, 1.764251, 6.486081]`; quarter/double scales produce matching extents.

Shark archetypes inherit `v20.player.playerstandardarmor` with no body-width or
height overrides. Rendering scales the fish model consistently, while the motor
scales the inherited Blockhead box/eye (1.25 width, 2.65 standing height at scale
one). Render identity and authored collision identity are therefore not equal.
The actual import report explicitly lists `boundingbox`, `crouchboundingbox` and
`proneboundingbox` among unsupported fields, alongside density, drag, step/jump
and inventory fields. Numeric authored boxes are not retained in that generated
report. This audit does not infer them from visual model extents.

Shark port gaps recorded in its repository port: mouth grab and delayed kill,
random/Cool Shark variants, automatic selection of its authored swim loop,
hidden hole brick, and its authored large collision box. Swim/flop speeds are
documented implementation choices, not exact original movement evidence.
Zombie gaps recorded in its port: infected-name prefix and the special marking
between zombies on different sides. Neither port nor this body-loading repair
is described as complete v20 parity. Other imported player archetypes found in
the actual bundle (Slayer frozen player and weapon movement variants) inherit
the standard body; they have not each received a new behavioral acceptance test
in this stability lane.

## Bounded video review

The supplied `Blocklandreimagined-v0.2.0-windows 2026.10.02 - 21.46.41.06.mp4`
is 3.899 seconds. A single four-frame contact sheet at one frame per second
shows a portal view changing when the third-person body appears near the
opening. That is consistent with the camera/presentation report; it supplies
neither precise world state nor a controlled before/after equivalence test.
No visible game or interactive input was used.

## Validation commands

- `cargo test -p bri-crash --test capture` — 2 passed.
- `cargo clippy -p bri-crash --all-targets -- -D warnings` — passed.
- `cargo test -p bri-crash --release --no-run --message-format=json` followed by
  stripping a copied `capture` executable and running it — 2 passed; 15 raw
  frames retained in a separate deliberate-panic report.
- `BRI_CONTENT=/Users/maxhastings/Documents/BlockReImagined/content cargo test -p bri-chaos --test bot_firefight -- --include-ignored --nocapture` — 1 passed.
- `BRI_TEST_DUPLICATOR_CONTENT=/Users/maxhastings/Documents/BlockReImagined/content cargo test -p bri-addon-import --test ports duplicator_selects_after -- --nocapture` — New Duplicator lifecycle passed (1).
- `cargo test -p bri-addon-import --test ports duplorcator_selects_after_map_adoption -- --exact --nocapture` — classic lifecycle passed (1).
- `cargo test -p bri-ui --lib admin -- --nocapture` — 7 passed, including both authority deadline regressions.
- `cargo test -p bri-client --lib invisible_noncolliding_bricks_get_visible_placement_geometry -- --nocapture` — 1 passed.
- Built client lib test binary: exact synthetic body reload test — 1 passed with
  its missing-license warning removed; `vehicle_camera::tests::` — 10 passed,
  1 content test ignored. Its separately attempted content test failed for the
  missing hardcoded fixture path described above.
- `cargo test -p bri-client --lib motion::crossing_tests:: -- --nocapture` — final
  5 passed (10.57 seconds), after the vehicle repair/shared predicate extraction.
- `BRI_CONTENT=/Users/maxhastings/Documents/BlockReImagined/content cargo test -p bri-client --test shark_body -- --include-ignored --nocapture` — 1 passed.
- `rustfmt --check --edition 2024` on owned crash/test files and preserved
  Duplicator/firefight tests — passed.
- Scoped client/crash clippy initially found the new nested cast conditional,
  now repaired. The latest rerun stops at concurrent NPC-lane
  `collapsible_if` at `session/events.rs:512` (bounded objective-input eviction)
  and `objectives.rs:905` (context casting); the owner was notified. Those were
  concurrent source snapshots, not a final combined gate result. Full client/crash clippy is not yet claimed as passed.

The full combined gate, Windows CI, release artifacts and Maxwell's playtests
remain root-owned integration/handoff work. A release handoff must still state
that the reported firefight crash has not been causally fixed.


## Candidate workflow symbol retention

The owned macOS/Linux release workflow changes keep debug information separate
from distributable content. Linux copies the exact unstripped client, importer
and server ELF files before `package_playtest.sh` runs `strip --strip-debug` in
place. macOS retains client/importer executables and generated dSYMs, asserts
executable/dSYM UUID equality, then verifies UUIDs in the extracted signed app.
Each 90-day symbol artifact includes `SYMBOLS.json` with the exact checked-out
source SHA, version, CI run, compiler, per-file byte counts and SHA-256 hashes;
the macOS manifest also records executable Mach-O UUIDs. Windows already retains
release PDBs in its separate symbol artifact. No original content is retained by
these new artifacts, and no release publishing or trigger behavior was changed.

Both candidate workflows now run `cargo test --release --locked -p bri-crash
--test capture`. Local macOS rerun passed two tests (0.04 seconds). Local
`dsymutil target/release/deps/capture-23977e6eda25a336 -o
artifacts/sol-stability/capture-release.dSYM` succeeded; `dwarfdump --uuid` on
that executable and bundle reports matching UUID
`617132B7-21B1-3250-A511-D578D0B8DD3A` (arm64). Workflow YAML parsing, every
embedded `bash -n` and Python `ast.parse`, and touched-file whitespace passed.
This is local release-test/dSYM and syntax evidence. Actual macOS/Linux candidate
CI symbol artifacts, extracted-package identity verification, and Windows/Linux
native crash-capture execution remain pending. These diagnostics do not fix or
identify the reported firefight panic.


Independent objective review found an omitted child reaction: a Target-scope
variable action dispatches `onRuleVariableChanged` on the actual named target,
while grounding previously checked only the source. The foundation owner added
conservative rejection. Added an independent ordinary-control regression here:
the supported two-step named-target chain selects its writer and scores 17;
adding a target child that resets progress and hides the latch instead scores 0,
never selects the writer, and keeps the latch visible. This establishes whole
action rejection rather than executing a supported prefix. No runtime source
was edited by this lane, and there is no claimed failed-before executable test.
Full `cargo test -p bri-chaos --test bot_navigation_spike -- --nocapture` now
passes seven in 0.26 seconds. A related source/target distinction in timer/score
reaction observation was sent to the foundation owner for assessment.

Read-only budget review confirms cumulative grounding target 128/term 4096/
text 64 KiB caps are checked before per-target context clones, program snapshots
are charged before cloning, retained source rows cap 256, sources cap 64, and
facts cap 128. The event world's named-resolution vector itself remains bounded
at 4096 before rejection. Pure search has separate node 256/depth 12/action 32/
candidate 4096 limits; one NPC snapshot is admitted per tick via round robin.
These source checks describe bounds, not a measured all-workload performance
claim. Real execution still owns permissions, scheduling, IF evaluation and
physical activation; projections do not execute state.

2026-10-03 independent shipping-recipe follow-up: normal `/rulelab race` uses
an ordinary PackageCommand (the first Chat-only harness input was corrected).
A standard Blockhead NPC in the host-created MiniGame completes the three
checkpoint laps through physical region entry, selects all three actual bricks,
and produces canonical scores 0/1/2/3. Tracing records `winRound -> Player ...
ran`; another five seconds retains score 3. No synthetic win, planner goal,
movement transform or recipe mutation is injected. Full final suite now passes
8 in 0.26 seconds, log `artifacts/sol-stability/navigation-final.log`.

Read-only combined combat review sent three source defects to the owners: live
hostile/alive/resting policy must be rechecked after motion, actual hitscan misses
cannot authorize a truncated safety ray, and charged images with mixed authored
fire edges cannot use a release-only hold shortcut. Owners added live checks,
lost-sight gate continuity, actual intended-body ray intersection and conservative
mixed-charge graph rejection. Normal queue removal and hand-image restart abort
an unsafe charge before the weapon advance, avoiding its release shot. Mounted,
portal-carried and explicit unknown-script executors remain scope boundaries.
Focused owner/independent combat validation is still being consolidated; this
review makes no all-weapon-family parity claim.


Independent `cargo test -p bri-sim --lib hand_combat::tests:: -- --nocapture`
passes four (0.01 seconds), including shared-budget fairness/reserve, mixed
charge-graph rejection and charge-hold/abort state logic. Evidence is ignored
`artifacts/sol-stability/combat-independent.log`. These pure state/allowance
checks do not substitute for the combat owner's current actual post-movement
frame regressions. Review also asked that the release-only proof cover an
initial onFire mount state, which needs no incoming transition edge.


Combat owner subsequently closed the initial onFire mount-state bypass and
added an ordinary-movement actual-ray-frame regression: a previously clear shot
is rejected after the live shooter enters a noncolliding/raycast wall; actual
ray misses also reject. The source review confirms `step_weapons` refreshes all
frames before checking every bot, including held triggers with no queued edge.
The frame regression exercises the validator with real motion; it is not a
standalone full-NPC held-trigger integration test. Foundation's final combined
library/chaos/clippy rerun includes the latest owner changes. The earlier frozen
library log has events 1/sim 150 passed +1 ignored; alltargets sim/events/chaos clippy
passed 10.61 seconds, including owned water/navigation source. Latest combat result
must be read from the owner's final rerun before claiming its test pass.
