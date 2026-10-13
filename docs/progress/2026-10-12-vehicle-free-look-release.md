# 2026-10-12 Vehicle free look springs back in third person

Max (v0.2.10): seated in a Jeep, holding Z and looking round, then letting go
left the view stuck where he looked; v20 swung it back ahead.

Cause: with strafe steering off (the default), the Jeep's driver is a mouse
driver (`SeatLook::MouseDriver`), and `Controls::advance_head` only eased that
head back in first person. The vehicle chase camera is swung by the same head
turn, so in third person it never came back.

The third-person exemption came from a direct reading of `Player::updateMove`
(blocklandv20.exe 0x5aeae3 to 0x5aeb27): with a vehicle control object, both
halvings are skipped when the controlling connection is not `isFirstPerson`
(0x526d60 reads `mCameraPos`, +0x364). Re-reading the exe, `mCameraPos` is
written only by `GameConnection::getControlCameraTransform` (0x592790, the
writes at 0x59288a to 0x5928ca), which moves it toward the process's own
first-person global (0x77026c). On a server a client joined, that connection
never followed the client's view, so the server kept halving the head and its
control-object update brought the client's view back ahead. That matches what
Max remembers. Inferred, not traced end to end: the listen-server host's own
case depends on the shared global.

Change: a mouse driver's free-look turn and pitch now halve every 32 ms tick
after Z is released in both views. Passengers and on-foot free look are
unchanged (passengers already returned in either view; on foot only third
person free looks and it already returned). Strafe drivers still never return.

Evidence: `cargo test -p bri-client --lib controls` (the renamed test
`a_mouse_driver_steers_and_free_look_springs_back` now checks the third-person
return, and `freelook_never_changes_body_facing_and_zoom_is_held` covers on
foot).

Next: Max to confirm the feel in a Jeep in both views.
