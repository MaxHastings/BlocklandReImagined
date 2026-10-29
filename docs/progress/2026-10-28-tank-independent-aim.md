# 2026-10-28 Independent tank turret heading

Maxwell's photographed report asks for tank and turret rotation to remain
separate while another player drives. This is an intentional improvement over
the retail behavior he also described as janky, not a claim of exact v20 parity.

The client added every change of hull heading to the gunner's world yaw in
`App::tick`. Both authoritative `Session::vehicle_input` and immediate local
`ClientVehicles::aim_locally` already subtract the hull heading when deriving
turret-relative yaw, so the extra carry undid that independence. Remove the
carry, its otherwise unused `Controls::carry_yaw` API, and the now write-only
last-hull-heading bookkeeping. Keep initial barrel
handover and old-seat input rejection. Hull pitch/roll, physical turret mounts,
aim limits and unoccupied turret behavior retain their existing mechanisms;
this correction stabilizes the controlled turret's heading, not its complete
orientation over uneven ground. No new seam, wire, save or content schema.

Extend the existing two-app LAN seat-change regression: after the guest takes
the gun back, a second player boards the driver's seat and drives left/right
while the guest supplies no look input. Require actual hull rotation, unchanged
gunner yaw/pitch, and both peers' replicated barrels at the held world heading.
This exercises the App path that the existing crew-link unit simulation bypasses.

Started on clean, merged warm `afghan-tank-navigation`, but another thread began
player-list edits there during compilation. The partial build saw a PlayerRow
API mismatch; it is not evidence of a turret failure or a baseline-main defect.
Only this task's test file was transferred to a dedicated worktree and restored
in the shared warm checkout; other-thread edits were preserved. Work continues
on `codex/tank-world-aim` from origin/main `f891e5aa8`.

## Validation and handoff

- Client formatting and `git diff --check` pass.
- `cargo build --profile playtest -p bri-client --bin bri-client` passes
  (13m35s cold after worktree isolation).
- Subprocess-waited `bri-client --check` against the main checkout's generated
  content exits zero: 21 maps, 1,396 bricks, 35/35 save pictures and 14 prepared
  lighting parameter maps. Its existing 227 fallbacks and two Butterfly Knife
  health warnings remain; no content was changed. The first direct GUI-subsystem
  shell invocation closed its output pipe prematurely; waiting through Python
  corrected the invocation and produced the complete exit-zero result.
- The extended two-App test initially failed its *actual hull motion* condition.
  Diagnostic authoritative poses established that its fixed frame-count driving
  left the small synthetic map's finite floor (y=-4,902, no wheel contact), rather
  than exposing a turret-aim failure. It now gives only this temporary synthetic
  Slate fixture driving ground sized from max_speed and the two ten-second turn
  budgets, and waits for observed signed hull turns over 0.15 rad with wheel
  support. Aim assertions were preserved. The corrected synthetic test passes
  (1/1, 42.81s). Logs: ignored `artifacts/tank-regression-*.log`.

Commands for the corrected test (same `playtest` profile):

```powershell
cargo test --profile playtest -p bri-client --test vehicle_first_person the_tank_turret_keeps_its_aim_through_seat_changes_for_gunner_and_watcher::synthetic -- --exact --nocapture
# The generated-content variant uses BRI_CONTENT pointed at existing native content:
cargo test --profile playtest -p bri-client --test vehicle_first_person the_tank_turret_keeps_its_aim_through_seat_changes_for_gunner_and_watcher::release_content -- --exact --ignored --nocapture
cargo test --profile playtest -p bri-client --lib --locked
cargo clippy --profile playtest -p bri-client --all-targets --locked -- -D warnings
```

Generated-content regression, unit and Clippy results are pending as this test
correction is committed; final evidence belongs to PR #393 and the release
coordinator's receipt. Maxwell's other chat explicitly authorized coordination
and owns the combined v0.4.1 merge/publication. This thread dispatches no release
workflow and does not independently merge main. No interactive game was launched
or controlled; the two-App test is offscreen.
Maxwell owns interactive acceptance; the full alpha contract remains open.
