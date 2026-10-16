# 2026-10-16 Contact-qualified rest for dropped items

Root granted a corrective atom from published
`5956e3a383a1c241bd5192a3aa8cc4f358f2e503`: only the weapons drop-motion loop,
new weapons/client dropped_item_motion tests and this note. Earlier read-only
diagnosis is preserved at `6adec645f91cc6886b16d3d1a84665989173bdda`.
This fixes the demonstrated straight-up stock-tool apex defect; the reporter's
exact aim, item and mode remain unknown. Movement/slope reports are separate.

## Change

The drop loop formerly skipped gravity whenever total speed squared was below
1e-6. An ordinary upward Hammer throw reaches approximately 0.000003 units/s at
its apex and consequently remained airborne while authority ticks continued.
The client displayed that same stopped state.

Low speed now sleeps only when a fresh point/box sweep finds an actual face
opposing the existing net gravity/buoyancy force. The probe crosses twice the
existing .002 collision skin. An unsupported item receives the existing gravity
step, including at exactly zero speed; removed support and changed buoyancy are
rechecked each tick. Supported rest keeps its position and zero velocity.
This adds at most one support sweep per slow drop per step, bounded by the
existing drop budget. There is no new persistent sleep state or identity.

Launch velocity, buoyancy calculation, bounce/friction, pickup and expiration
remain unchanged. No ghosts, schemas, wire, general physics or other production
paths changed. Client ghost changes were not needed: ordinary App regressions
show both authoritative and presented descent after the authority correction.
No original contact-audio repeat/promotion policy is inferred or bound.

## Retained regression evidence

The five weapons cases cover an ordinary upward tool throw, exact/near-zero
airborne point and box drops, stable supported rest and support removal,
buoyancy lifting from support, and moving-drop bounce/friction followed by
settling, pickup and timeout. Before the fix, four fail and the moving-drop
control passes. All five pass with the fix.

Two client cases use the actual headless App, native synthetic content, local
authority/network worker, ordinary Q equip, Look and DropTool, then Q open/close
while simulation time advances. Both SP and LAN hosting must show authoritative
and CPU-presented descent by over two units after the peak. The test finishes
before the returning Hammer can be picked up by its stationary thrower. Neither
case opens a visible window, renders a GPU image or sends OS input. This is
local listen-host evidence, not a remote-peer or interactive acceptance claim.

Both final client cases fail on the exact released runtime at Y=4.4267936,
vy=0.0000030100346. Baseline verification temporarily installs the released
runtime, runs the unchanged final tests, and restores the fixed source bytes in
a finally block. The fixed-source hash is retained for the final comparison.

Initial fixture compilation used nonexistent testing::empty/TargetId::World;
corrected to the existing synthetic pack and Map target. An initial App driver
assumed Q remained open after dropping; corrected before the actual apex red.
The first green attempt waited past descent into legitimate automatic pickup;
the retained test now finishes on the descent proof. Final baseline reruns use
that same final predicate and still produce four weapons and two client reds.
These setup errors are retained in logs, not represented as production failures.

## Commands and receipts

All Cargo calls use two jobs, debug=0, incremental=0 and CARGO_TARGET_DIR unset.
No original/private assets, source installations or other checkouts changed.

- Final baseline `cargo test -p bri-weapons --test dropped_item_motion --locked`:
  1 passed / 4 failed, expected red. Client equivalent: 0 passed / 2 failed,
  expected apex red. `target/drop-apex-final-baseline-bri-{weapons,client}.log`.
- Targeted corrected weapons: 5 passed / 0 failed; corrected App: 2 passed /
  0 failed. `target/drop-apex-weapons-green.log` and
  `target/drop-apex-client-green-final.log` (full focused run covers final source).
- `python tools/check.py quick --crates bri-weapons,bri-client`: four steps,
  68 seconds, 597 passed / 0 failed / 76 existing ignored. Client 584, weapons 13.
  `target/drop-apex-quick.log`.
- `python tools/check.py focused --crates bri-weapons,bri-client --depth 1
  --tests bri-client:dropped_item_motion`: all 13 steps passed, actual exit zero,
  1,777 seconds. Both changed crates pass all-targets warnings-denied Clippy;
  changed crates and direct dependents pass their tier-defined suites, plus
  the explicit App target (2 passed). Across 232 result summaries: 2,663 passed /
  0 failed / 278 existing ignored. Final five weapons cases all pass, including
  bounce-to-rest stability, and existing portal/buoyancy controls pass.
  Receipt: `target/drop-apex-focused.log`. Production/test source SHA256s stayed
  unchanged during the final focused run (`target/drop-apex-frozen-source.json`).
- `git diff --check` passes. No full gate, main integration, package publication
  or interactive playtest closure is claimed here.

Root owns combined integration and release validation. Existing B1/SP/contact
and movement branches/diagnostic receipts remain preserved.
