# 2026-10-28 Independent tank turret heading

Maxwell's photographed report asks for tank and turret rotation to remain
separate while another player drives. This is an intentional improvement over
the retail behavior he also described as janky, not a claim of exact v20 parity.

The client added every change of hull heading to the gunner's world yaw in
`App::tick`. Both authoritative `Session::vehicle_input` and immediate local
`ClientVehicles::aim_locally` already subtract the hull heading when deriving
turret-relative yaw, so the extra carry undid that independence. Remove the
carry and its otherwise unused `Controls::carry_yaw` API. Keep initial barrel
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

Validation and private handoff results will be appended before completion.
Maxwell owns interactive acceptance; the full alpha contract remains open.
