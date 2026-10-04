# 2026-10-02 Sol stability audit and diagnostics

This lane preserves the existing v0.2.1 playtest changes and records their
evidence in [the stability audit](../audits/v0.2.1-stability.md). It does not
change the published v0.2.0 release, commit/push anything, or mark the alpha
contract complete. No game window or interactive input was used; original and
primary converted content were only read.

The supplied Windows Bedroom crash remains **OPEN, cause unknown**. The actual
report is a main-thread scalar float clamp with two NaN bounds and no addresses
in its unknown frames. The cleaned headless mixed-bot test passed two 45-second
cases (Gun unchanged: 926 projectiles; Gun-to-Gravity-Gun spawn edit: 198) in
46.57 seconds. It verifies sixteen spawned bots and finite MessagePack
snapshots/motion, but does not cover client rendering, audio or Windows.
No speculative NaN guards were added or described as a crash fix.

Crash capture now writes an independent native frame section: raw return IP,
module, loaded base and relative offset, plus the existing Rust backtrace and
executable path. Windows uses stack capture/image memory mapping/kernel32 module
name APIs with its existing dependency; Mac/Linux use backtrace/dladdr. Alternate
Debug formatting of std::Backtrace alone does not expose raw stored frame IPs.
Tests require multiple real module mappings and address/offset arithmetic.
Apple silicon debug capture passes (2 tests); warnings-denied crash clippy
passes. A copied release test binary stripped with `/usr/bin/strip` passes
both tests; its deliberate panic still retains fifteen native frames despite
symbolic frames that all resolve to `__mh_execute_header`. Matching build symbols
and Windows/Linux runtime validation remain required integration evidence.

Reviewed and independently reran the preserved Duplicator lifecycle tests:
the actual converted New Duplicator and classic fixture both select through
short clicks before and after map adoption. The session repair initializes
package player defaults on adoption. CPU preview geometry for copied invisible,
noncolliding bricks passes without altering planted flags. Admin request tests
pass (7 filtered tests): ack alone retains the authority deadline, missing/stale
lists time out after 45 seconds, and a correlated list completes without a false
timeout. These are scoped regressions rather than blanket feature completion.

Added ignored real-content `crates/client/tests/shark_body.rs`. It loads the real
base avatar role and Bot_Shark's original converted shape, validates its
12-node/6-object rig and retained swim/bite sequences, compares the original
blank/black75 material pixels, and builds 1698 finite posed vertices/indices at
normal, quarter and double scale. Root-pose extent at scale one is
`[3.0759225, 1.764251, 6.486081]`; visual scaling matches. Synthetic enable/disable
reload separately verifies replacement of AvatarAssets and clearing old meshes;
its made-up Fish fixture now declares its CC0 license so its package parses.
Shark's collision still inherits the Blockhead box, and the port's mouth grab,
variants, automatic swim animation and hidden hole are incomplete. Zombie has
infected-name and cross-side marking gaps. These are explicitly inventoried,
not claimed as full body/NPC parity or navigation acceptance.

Bounded review of Maxwell's 3.899-second portal video extracted four frames.
They show the view changing when the third-person body appears near the portal;
they do not supply a reproducible world state or prove video equivalence. Four
CPU doorway/floor/camera regressions passed in 10.23 seconds.

The review found the vehicle driver's camera still took an uncut wall ray while
walking/observer cameras used clipped portal geometry. Root authorized the narrow
repair. A new production-path regression first failed: the camera stayed at
`[23.25, 1.575, -4.21]` instead of its carried `[107.25, 1.575, -40.25]`.
Extracted `CollisionMirror::portal_camera_hit` from the existing camera sphere
cast so drivers keep its true surface normal and their authored back-off;
the player position wrapper keeps its original 0.02 margin. Driver rays now use
that clipped query near an opening/closed pane and keep their indexed ordinary
query elsewhere. The focused test passes for backing-wall traversal, a wall in
the exit room, its normal, a missing partner and exact ordinary-hit preservation.
The shared near-portal predicate now serves both camera routes. Final five
camera crossing tests pass (10.57 seconds); ten baseline vehicle camera tests
pass, including authored back-off and easing. The existing real-content vehicle
eye test was attempted but fails on its hardcoded absent worktree `content/`
path; it ignores BRI_CONTENT. Root subsequently supplied a read-only symlink
to main content, and the exact test passed (1, 0.61 seconds). This establishes
that the initial failure was missing harness input; no test code was changed. The synthetic reload
rerun passes with its invalid Fish license warning gone; the actual Shark
regression also passes after final source changes.

Scoped client/crash clippy found a nested cast conditional in this lane, now
repaired. The latest rerun stops at concurrent NPC-lane collapsible-if findings
at session/events.rs:512 (objective-input eviction) and objectives.rs:905
(objective context casting); their owner was notified. This is a snapshot of
the then-current concurrent source, not a final gate result. A complete combined clippy pass
is still required before landing. All owned production/test source is frozen
for root integration unless that final check reveals a new owned issue.

Commands, exact scope/limits and local artifact paths are in the audit. Final
gate, Windows CI, packages and interactive handoff remain root-owned. The
release's work plan must retain the causal firefight crash as OPEN unless a
subsequent reproduction and repair establish otherwise.


Root verification environment: Apple's `python3` selected Python 3.9 and the
tool-test run failed importing `tomllib`. Root reran
`/opt/homebrew/bin/python3 -m unittest discover -s tools -p 'test_*.py'`:
16 passed in 0.160 seconds. This matches the documented Homebrew Python 3.14
PATH requirement for the gate. Root supplied the read-only content symlink;
the source checkout is still uncommitted. These root-reported results are
integration environment evidence, not a new source defect.


Release symbol retention: Linux's existing packager strips its built ELF files
in place. The Linux release workflow now copies the exact client/importer/server
executables before packaging, writes a source-commit/version/run/compiler and
SHA-256 manifest, and keeps a separate 90-day CI symbol artifact. The macOS
workflow retains client/importer executables and `dsymutil` dSYMs, verifies their
Mach-O UUIDs, and checks the extracted signed package still carries those UUIDs.
Both candidate workflows run `cargo test --release --locked -p bri-crash --test
capture` before packaging. No original content enters these symbol artifacts;
release triggers, publishing behavior and the published v0.2.0 are unchanged.

Local release capture rerun: 2 passed, 0.04 seconds. A local release capture
executable and its generated dSYM both report UUID
`617132B7-21B1-3250-A511-D578D0B8DD3A` (arm64). Both workflow YAML files parse;
all embedded shell/Python snippets pass syntax checks. This proves local test
symbol identity and workflow syntax, not execution of the full macOS or Linux
candidate workflow. Actual candidate binaries/artifacts and Windows/Linux
native-capture execution remain pending CI integration evidence. No commit or
push was performed in this lane.

2026-10-03 follow-up: completed a repository-wide dynamic-bound clamp inventory
and traced animation, interpolation, controls, bot, motor, vehicle, audio,
UI/event and geometry validation. No normal packaged input admitting NaN into
those bounds was identified. Bot bite bounds still derive from runtime physics
feet, and numerical collision/dependency corruption remains unexcluded. Direct
Rust use of the public audio helper with invalid NaN distances reproduces the
assertion, while current sound adapters use validated metadata or finite
constants. No speculative audio or blanket NaN guard was added. The audit now
records exact boundary evidence, unknowns and a diagnostic-candidate disposition;
the original crash remains OPEN.

Found a distinct reachable finite-bounds reversal: schema-valid thin water
(width 0.25) makes the old half-unit swimmer inset min 2.5/max 1.75. Root granted
`sim/water.rs`; focused test failed before, and the min(halfwidth,0.5) inset fix
passes all 4 water tests, retaining wide-water behavior. This does not explain
the reported NaN panic. Windows release workflow now runs release crash capture,
and a Windows-only copied executable regression verifies native mapping without
adjacent PDBs (embedded original PDB resolution is explicitly not excluded).
Local release capture 2 passed; Windows execution pending CI.
