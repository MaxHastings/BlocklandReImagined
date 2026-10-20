> **Historical note. Written 2026-10-06 (the file's date in the cloud project folder, `audits/bug-patterns-2026-10-06.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Bug patterns, round 2 (2026-10-06)

Builds on the 2026-09-28 pass (`docs/audits/bug-patterns.md` on main, thread
"Squash bug patterns proactively"). That pass found five habits (screen and
server disagree, guessed v20 rules, host-only testing, hard stops, no budgets)
and added request deadlines, the command and event fuzzers and the panic fuse.
Those mostly held: none of the bugs below is a crash on bad input, a stuck
screen or a runaway loop. The bugs that got through since are a different kind.

Sources read: main `a00fa5f` back to v0.2.0, every `docs/progress/` entry from
10-01 to 10-06, the unmerged branches `claude/false-placement-errors-29vu1c`
(`5217ce4`) and `claude/project-thread-44eihq` (`7bd40dd`), the 09-27 bug table
and the v0.1.15 catch-up. Code only; nothing was run.

## The bugs

| Bug (version found) | Cause in one line | Pattern |
|---|---|---|
| Bricks refused as Buried/Stuck far more than v20 (v0.2.4) | We refused any 2 mm map contact and any overlap with a player; v20 only casts a few rays at interiors and never checks players | 1 |
| Coffin lid closes onto its open back from behind (v0.2.4) | Closing rule guessed from side clicked; v20's Support_Doors not in hand | 1, 4 |
| Doors refused every click (v0.2.0) | A "same size" guard silently refused open doors, which are wider | 1, 6 |
| Hammer and "indestructable", horse camera, relay floor (v0.1.x) | Rule written from memory, test locked it in | 1 |
| Tool wheel snaps back a slot (v0.2.4) | Host answers the equip before its next update; client put the old tool back in between | 2 |
| Spray can "outside palette" after Change Map (v0.2.0) | Palette not resent; client kept the old one | 2 |
| Pong paddles stay white after Load Bricks (before v0.2.0) | Event engine kept the colour count it saw at start | 2 |
| Reconnect joins "Blockland Server" (code review, v0.2.3) | One field held both the join address and the display name | 2 |
| Admin refresh stuck on old state (v0.2.1/v0.2.2) | Cached copy not replaced | 2 |
| Sitting first-person eye at standing height (v0.2.4) | Camera used the motion eye; the drawn body used the sit pose | 3 |
| Ragdoll vanish puff in the wrong place (v0.2.4) | Server puts the puff at the game body; the ragdoll is drawn somewhere else | 3 |
| Mirror rooms break up (v0.2.4) | A picture drawn for one eye shown to another | 3 |
| Save picture missing bricks (v0.2.3) | Picture and frame shared one GPU submission; the frame overwrote the picture | 3 |
| Foliage and far water not dimming at night (v0.2.0) | One render path missed the shared time-of-day input | 3 |
| Duplicator moves the old ghost (v0.2.4) | With a ghost out, the ghost got the brick keys before the held tool | 4 |
| Crouch while sitting crouches the view only (v0.2.4) | Same two-eye cause, only in the sit + crouch combination | 3, 4 |
| Shark join error (v0.2.0) | Shark borrows Bot_Hole's shape; only tested with Bot_Hole on | 4 |
| Lost-GPU crash on a friend's PC (v0.2.4) | wgpu's helper panics on a lost device before our recovery runs | 5 |
| Friend waits ~4 min on Loading Graphics (v0.2.4) | Source build without dxcompiler.dll, slow FXC compile | 5 |
| Gravity Gun effects refused on a busy PC (gate, v0.2.3) | GPU speed timed by wall clock, so load made it look slow | 5 |
| "Effects frame exceeds GPU budget" closed the game (code review, v0.2.3) | Renderer sized for two effect worlds, three exist | 5 |
| Add-On hook failures and Slayer bot-fill failures invisible (v0.2.4 work) | Failure dropped with no message | 6 |

Still open, same patterns:
- A seated player's shots still leave from standing eye height
  (`crates/sim/src/session/weapons.rs:342` uses `peer.player.eye()`). Pattern 3,
  noted in the sit fix and not fixed.
- Shark with Bot_Hole off is still untested (catch-up note). Pattern 4.
- The coffin rule is inferred, not read from v20. Pattern 1.

## The six patterns, and how each got past us

1. **Our rule is stricter than v20's, or a different rule.** The code refuses or
   behaves where v20 did not.
   *How it got past:* the tests were written from our own rule ("refuse when it
   overlaps"). Nothing checks the other side: that everything v20 allowed is
   still allowed. When the original script was not in hand, the rule was
   guessed and nobody flagged it as a guess.

2. **Two copies of one fact, and one goes stale for a while.** The client's
   copy and the host's copy, or a cached copy and the world.
   *How it got past:* tests check the end state once everything has settled. On
   loopback the reply and the update arrive almost together, so the gap a
   player sees for a frame or two never shows in a test.

3. **Two answers to "where is it?"** What is drawn and what is simulated, or two
   render paths, each with its own idea of the eye, the body or the input.
   *How it got past:* each path has its own test and passes alone. No test
   compares them.

4. **Combinations and sides.** Each feature works from a clean start; the bug
   needs two at once (a ghost out plus the Duplicator, sit plus crouch, Shark
   without Bot_Hole) or the other side (the coffin from behind).
   *How it got past:* tests start from a clean state with one feature. Add-On
   combinations exist only in Max's real set.

5. **Machines we don't have.** A lost GPU, a missing DLL, a slow shader
   compiler, a busy machine.
   *How it got past:* we test on Max's GPU and the cloud's software GPU, both
   healthy. Nothing simulates a lost device or a slow one.

6. **A quiet "no".** A click or request does nothing and says nothing.
   *How it got past:* the 09-28 pass turned hard stops into fallbacks, but
   nothing counts how often the game says no, so a rule that refuses too much
   looks the same as one that works.

**Underneath all six:** our tests check what we built, not what v20 does or
what a player sees frame to frame. Max is still the only one who plays, so he
finds them.

## Proactive checks, cheapest first

1. **Ban wgpu's panicking buffer helper in `clippy.toml`.** `clippy.toml`
   already bans `f32::clamp` for the same reason. Add
   `wgpu::util::DeviceExt::create_buffer_init` (pointing at
   `bri_render::BufferInit`), with an allow only in its own test and the weather
   example. Stops the lost-GPU crash coming back through a new call. Minutes of
   work, catches pattern 5.

2. **Count every "no".** One named counter per refusal reason (Buried, Stuck,
   Float, door swap refused, equip refused, Add-On hook failed), printed by the
   app soak and the save corpus and shown in the F3 readout. A rule that starts
   refusing much more shows up as a jump in a number. Cheap; catches 1 and 6.

3. **"v20 allowed it" replay.** Every save in `save-corpus.json` was built by
   hand in v20, so v20's plant check accepted each brick. Replay each save
   brick by brick through our normal plant check, in file order, on its map,
   and report refusals by reason. A high refusal rate is a false refusal. It
   would have caught Buried and Stuck before Max did. Runs inside the existing
   save-corpus step on the PC (needs the maps). Catches 1.

4. **"No snap back" watcher.** In the loopback client tests, record the
   selected tool, HUD values and held image every frame, and fail if one goes
   back to an older value without new input. Add a loopback option that holds
   updates a few ticks behind replies, so the gap is real. It would have caught
   the tool wheel. Catches 2.

5. **"One answer" table test.** For stand, crouch, sit, sit plus crouch, ride,
   swim and dead (ragdoll on), check that the first-person eye, the drawn
   body's Eye node and the server's shot origin agree, and that effects a body
   makes appear at the drawn body. It fails today on the seated shot origin,
   so it finds a real bug on its first run. Catches 3.

6. **Combination matrix.** Each held tool, times each starting state (ghost
   out, seated, in a vehicle, dead), times front and back clicks: send the
   brick keys, fire and click, and check who receives them. Plus each bundled
   Add-On enabled on its own (Shark without Bot_Hole). Table-driven, so new
   tools and states are one row. Catches 4.

7. **Lost-GPU drill.** In the offscreen tests (software GPU in the cloud),
   lose the device at a chosen frame, during a map load, an upload and a save
   picture, and require the game to recover within a few frames. Catches 5.

8. **Mark guesses.** Every v20 behaviour fix names the v20 script line or exe
   address it copies. Anything inferred (the coffin rule today) goes on one
   "inferred, check against v20" list in `docs/audits/v20-behaviour.md`. That
   list is the queue for the next v20 audit. Free to start; it prevents
   pattern 1 rather than detecting it.

Checks 1, 4, 5, 6 and 7 are hermetic and can be built and run in the cloud.
Checks 2 and 3 need the real content and saves for their full value, so they
run in the PC gate's save-corpus step, though the counters can land from the
cloud.
