> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-mirror-flicker.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: mirror tunnel flicker fix (f28e30fd on 498d7f58, claude/fix-mirror-recursion-dv7x1g)

Verdict: **passes. One test fix first:** tighten the threshold, see 3. Then merge that sha. Checking it is a quick look. If the merger wants no new sha, f28e30fd is still safe to merge; the test is just weak as a guard.

## 1. Deepest-first order: holds, with one assumption to pin

- The plan lists a parent before the planes seen in it (`Plan` doc, reflection.rs ~180). `render_views` draws in reverse index order (reflection.rs:1346).
- So in plane i's view, plane j has already drawn this frame exactly when j > i, whatever the graph: portals, mirrors, mixed, siblings or cousins. Depth plays no part; only the index does.
  - The new rule (`Echo(j)` with j < i becomes `Last`) is correct.
  - `Live(j)` always has j > i (a child), so it is fresh.
  - j == i is already Silver.
  - The player's view draws last, so its echoes stay live.
- **The assumption:** `render_views` draws in two passes, planes with their own sun shadows first and then the rest (reflection.rs:1347-1354). That is pure reverse order only while `own_shadows` is all true or all false. The prepare comment says it is ("fitting fails for every plane alike", ~1097-1103), and that's plausible.
  - If it were ever mixed, an `Echo(j)` with j > i could show a plane not yet drawn, which is this same bug again.
  - Not a blocker. In this PR or v0.2.7, add a `debug_assert!` that `own_shadows` is uniform, or have `slots` take the real draw order.

## 2. Shows::Last can't show another plane's or a resized picture: confirmed

- **Resized:** a size or setting change clears `targets` before `held` is built (`prepare` ~973-985, `set_settings` ~941). `held` covers only existing targets, so `last` is empty after any resize, and those views fall back to Silver/first-frame behaviour.
- **Another plane's picture:**
  - `last` maps a group key to the target that drew that group last frame (~981-1007).
  - `render_views` copies picture to previous **before** any plane draws (~1310-1318), so a live draw this frame can't overwrite what `Last` samples.
  - `Last` reads `previous` with that frame's own echo uniform (the `echoes` saved per target), so the framing matches the picture.
- **Minor, as before:** when several planes shared a group last frame, `find` takes the lowest target. That one is framed correctly but may be the same mirror seen along a different path. Echo already behaves this way, and nothing reads as undrawn.
- **Known effect, accepted:** deep echoes in a plane's view are now a frame late. On a camera cut (respawn, teleport) they show a far-away picture for one frame. That is tiny at tunnel depth, and Portal does the same.

## 3. The 1% threshold is not sized from the measurement: tighten it

- SIZE = 128, so 1% = 163 pixels.
- Main fails this test at 168 in frame 3, only 3% over the bar. The note's "two to three times this" comes from the lane's wider sweep (444 max), not from this walk.
- The fix's worst is 44 at Medium and 5 at High (lavapipe).
- So the bar sits almost on main's failing number and nearly 4× above the fix. A partial regression would pass.
- Ask:
  - Set the bar from the measurement, for example `SIZE * SIZE / 200` (82 pixels, about 2× the fix's worst and half of main's frame 3), with a comment giving both measured numbers and the GPU.
  - Collect every frame's count and assert on the worst one, printing all counts. The current loop stops at the first failure, so the record never shows main's worst frame.
  - If lavapipe vs a real GPU moves the residual (as the CI-only mirror sharpness test does), keep the bar and note the device. Don't loosen it.
- **Residual, worth a line in the hand-off:** 44 of 16,384 pixels is 0.27% of the screen at the vanishing point. The lane says it is texels too small to fill. If so, its share should shrink at real resolutions (inferred, not measured). If it toggles frame to frame, Max may still see a speck flicker at the far end at Medium. Ask the lane for the per-frame residual counts, and have Max look at the tunnel's end in the preview.

## 4. Cost: no regression

- `slots` adds one pass over the groups per view, with a linear `find` in `last`. That is at most MAX_MIRRORS by live targets, which is a handful.
- No new pass, texture, copy or bind group. `previous` was already copied each frame for any held target, and the slot table already binds every target's `previous` (~1015-1023).

## Notes outside the fix

- The progress note's "also answered" items (portal shadows, lamp picking per plane for v0.2.7, Low vs Best window light) are explanations only and change nothing in this PR. The lamp-per-plane item goes on the v0.2.7 list.

## Follow-ups at 7a8af5ec (merge the whole branch at this sha)

Verdict: **passes. Merge exactly 7a8af5ec.**

### 25dcc236: flicker test bar
- `MOST_UNDRAWN = SIZE²/200` (81). The comment cites main's worst (444), the fix's worst on this walk (3) and on a walk four times as fast (44). That puts the bar about 5× under main and about 2× over the fix's fast-walk worst, so it is sized from the measurement now.
- It asserts on the worst frame and prints every frame's count. Good.
- `debug_assert!` that `own_shadows` is all-or-nothing (reflection.rs ~1324) pins the draw-order assumption `slots` relies on. Good.

### 4e644811 + 7a8af5ec: held-item render flake
- **Cause:** plausible, and it fits the symptom. The Bedroom's Unified lighting comes from a compatibility bake on a worker. Frames are Classic until a frame polls it. Alone, it lands before the first capture; under load it can land between `third` and `reset`, which is exactly the failing assert ("GPU recreation changed static third-person item render", app_item_render.rs ~533). This was reasoned from the code, not reproduced, as the note says.
- **Complete enough:** `map_lighting_settled` (client/src/app/probes.rs:232) also covers world render ready, switchable texture sheets due, and the GPU scene being present. Shadow cascades are fitted every frame from the camera, so they don't land late (inferred). I found no other worker result that changes a drawn frame. Both tests wait once after the first `gpu_ready` and before any compared frame.
- **Bounded:** yes. `wait::until_one` with 45 s of game time once in game, plus the 300 s `STALL` guard if the app stops advancing. Each failure names "the map's lighting to settle". It can't hang past the gate's per-process deadline.
- **One small hardening (optional, v0.2.7 or this sha):** the wait comes only before the first capture. If a GPU reset ever made the lighting re-bake asynchronously, the `reset` capture would flake the same way. It passes alone today, so the reset path re-uploads synchronously (inferred). Asserting `app.map_lighting_settled()` right after the second `gpu_ready` (line ~530) would turn that case into a clear message instead of a pixel diff. It must be an assert, not a wait, so a real reset regression still fails.
- **Proof:** the next PC gate under load. If it flakes again, the cause was something else; report it rather than retrying.
