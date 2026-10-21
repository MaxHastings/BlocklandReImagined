> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-ragdoll-never-stops.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: ragdoll never stops for lag (a521921c)

Branch claude/fix-ragdoll-never-stops, one commit a521921c64de931c9c206f983ecc0490a3ff077b on main d068f06e.
Not run here: bri-client needs ALSA headers this container lacks. Findings are from reading the code; the mid-air point is shown by the PR's own test.

Verdict: FAIL, two must-fixes and one to finish Max's chat rule.

## 1. Must-fix: one slow frame freezes a ragdoll in mid-air
- `advance_physics` (client_code.rs ~405) settles on ANY single frame over 4 ms of wall time. There are no strikes now, so one hitch (OS preemption, a load, a GC in another thread) triggers it.
- `settle_oldest` (addon_physics.rs ~222) calls `sleep()` on the oldest moving group wherever it is. Rapier zeroes the velocity and the group hangs there until something hits or holds it.
- The PR's own test proves the freeze. Bodies 1–3 are falling from y=8 after 0.25 s, then stay at the same position.
- Player view: during a lag spike, a corpse thrown by a blast stops dead in the air for the rest of its 5 s. That is the same complaint as before ("ragdoll physics disabled by a spike"), on one body instead of all.
- Fix, same mechanism, no new constant: settle only a group that is already resting on something. For example, every body in the group touches the building or another body (it has an active contact pair), so laying it still is invisible. If no group qualifies, settle nothing that frame. The cost stays bounded by the Add-On's own limits (24 ragdolls × 9 parts, 5 s life). Test: a mid-air group is never settled; a grounded one is.

## 2. Must-fix: a lag spike can still switch the whole Ragdoll off (Stopped::Time)
- Every `frame` call runs under an 8 ms WALL-CLOCK epoch deadline (host.rs:107 `frame_time`, :655). If the game thread is descheduled for about 9 ms while inside the Add-On's frame, Wasmtime traps with Interrupt, then `Stopped::Time` ("it took too long to respond"), and the Add-On stops for the session.
- That is exactly "disabled because of a spike of lag". Fuel (`frame_fuel`) already bounds the work deterministically. The wall clock is only a backstop for a hang.
- Fix: make the frame wall-clock deadline a hang backstop that a hitch cannot reach (the module doc at host.rs:11 already calls it a backstop), or count consecutive timeouts the way gpu_strikes does. Do NOT skip and continue after a trap, because the module's memory may be half-updated. Test: a frame that stalls for a lag-sized moment (e.g. via a test-only slow host call) does not stop the Add-On, and a real hang still does.
- Other stops checked: Cpu (fuel, deterministic, not lag), Budget for bodies/joints/calls (the Ragdoll stays under them by design: 24 × 9 = 216 bodies), Gpu (the Ragdoll draws no layer; it poses avatars), Misuse/Crashed (bugs, not lag). None of these depend on lag.

## 3. Finish Max's rule "never show engine diagnostics in player chat"
Only the "X stopped" lines moved. These still go to chat through `messages` (client_code.rs):
- "Add-On X has code that cannot run" (~151)
- "Could not read <trust file>" (~203)
- "Add-On code cannot run on this PC" (~212)
- "X: name does not play" (sound decode, ~266)
- each Add-On's own log lines "Name: line" (~465; the existing test expects "Spinning Cube: spinning cube ready" in chat)

Keep only what the player must act on in chat: "code is off: you have not trusted this server" (~235). Move the rest to bri_console. On the question asked: moving "stopped" to the console does hide that an Add-On was refused or stopped, but Max asked for exactly that. A player-facing Add-On status (e.g. in the Add-Ons list) is a v0.2.7 item, not a reason to keep chat.

## Answers to the coordinator's questions
- Falling behind: no hidden stop. One group per over-budget frame. The Ragdoll's maximum is 24 groups and each lives 5 s, so it catches up within about 24 over-budget frames. Body ids are monotonic per session (host.rs ~1478 `next_body`), so the lowest id in the BTreeMap really is the oldest.
- Oscillation: a woken old group is the first to be re-settled on the next over-budget frame. Under sustained load, a hit on an old ragdoll therefore moves it for about one frame. That is a degrade, not an oscillation. With fix 1 (settle only resting groups) it becomes invisible, because a flying group is never re-frozen. Rapier also wakes a sleeping island when an awake body touches it (a new corpse landing on a pile). That costs frames but is bounded as above.
- Everything else (deleting Stopped::Physics, report_physics_time, physics_strikes; docs) is clean. No new constant.

## Re-review 715cfb71 (delta on a521921c): FAIL, one small must-fix left; rest PASS
Ran here (container now has the audio/udev headers): bri-client addon_physics 16 passed; bri-client-sandbox non-GPU tests pass, including a_frame_stalled_far_past_a_display_frame_carries_on, bulk_memory_loops_are_stopped_by_the_clock and wall_clock_time_is_a_budget_too. The 5 failing tests are offscreen GPU renders; this container has no adapter.

### Must-fix: "lies on the world" also counts a WALL, so a corpse can freeze stuck to a wall
- `lies_on_world` (addon_physics.rs ~220) accepts any contact with any fixed collider within allowed_linear_error. A brick wall is a fixed collider. So a corpse blasted against a wall, still in the air but touching it at that moment, is "on the world" and gets put to sleep there: pinned to the wall for up to 5 s under sustained load. It's the mid-air freeze again, rarer.
- Fix: count a contact only when the fixed surface holds the body up, i.e. the contact normal (surface → body) points up as much as a floor does. Reuse the motor's own floor test (bri-motor torque.rs:1015 `FLOOR_DOT`), no new constant. Add a test: a group touching only a wall is not settled.

### Checked and fine
- Airborne groups: never settled (new test proves it). Group dedup via `seen` is right; still at most one group per over-budget frame.
- hang_time = 1 s for init and frame. The epoch deadline is set at the start of each call (host.rs:655), so only time spent INSIDE the Add-On's own call counts. Shader compiles and map loads run in the game thread outside the call, so they can't trip it. Host functions take no locks or blocking waits (no lock/recv/block_on in host.rs). Only an OS stall of more than 1 s in the middle of a short call (heavy paging) could trip it, and that is hang territory. Answer: no, the same bug isn't back in a new place.
- Fuel still bounds the work; red-team clock tests still stop bulk-memory loops.
- Chat: only "you have not trusted this server" stays, which is right because it's the only line the player can act on. None of the console-only lines (elevated code off, cannot run, trust file read error, cannot run on this PC, sound, skins, Add-On logs) has a player action. A visible Add-On status in the Add-Ons list → v0.2.7.

## Round 3: 807ebc3d (delta on 715cfb71). PASS. Merge 807ebc3d7f2ca7fe2504323a25c69039cdcc7a4c.
- The wall must-fix is fixed. A contact counts only when the normal, turned to point from the surface into the body, has y > FLOOR_DOT. Rapier's manifold normal points collider1 → collider2, so negating it when the body is collider1 is the right sign.
- FLOOR_DOT is the motor's one definition (bri-motor torque.rs:920, v20 updatePos 0x5B173C), re-exported through bri_sim::player. No new constant.
- New test settling_never_sticks_a_body_to_a_wall; the floor test still passes. Ran here: bri-client addon_physics 17 passed.
- Earlier rounds stand: hang_time is the only clock, chat holds only the trust line.
