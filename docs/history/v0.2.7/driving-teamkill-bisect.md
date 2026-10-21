> **Historical note. Written 2026-10-08 (the file's date in the cloud project folder, `v0.2.7/driving-teamkill-bisect.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# v0.2.6 gate failures: driving and teamkill bisect (Linux, cloud)

## Driving: pinned and fixed ✅
- **First bad commit:** dc7d25f "Route grid: every floor of a column" (navigation branch).
  - Green at 4571b9c (the stack before navigation) and at 57ecb29 (its parent).
  - Red at dc7d25f, at f1699c6, and at the merge 5b4fc664.
  - Found with `git bisect run` over c514ff4e..03a19527, using the 4 driving tests.
- **Cause:** `sample()` in nav.rs (line 1227 at e05a1383) skipped any hit whose ray started "inside" something.
  - A chassis's route start sits below the ground, because the wheels sink into the suspension. Its short reach (0.2) put the ray's start exactly on the ground's top face, where the point test counts as inside.
  - So the vehicle's own cell had no floor and every drive search came back Nowhere. I confirmed this with debug prints: the start cells showed `inside=true`, `clear=true`, `place=Some`, and the chassis didn't move for 100+ ticks.
- **Fix:** the release lane's ac50d6e moves the inside test a hair (`SETTLE_GAP`) above the ray's start. My equivalent, `!(distance > 0.0 && inside(origin))`, also passed all 4 driving tests and all 30 nav tests at e05a1383 ([driving-fix.patch](driving-fix.patch)). ac50d6e is on the stack, and the lane reports 30/30 driving and interaction tests on the PC.
- Leftover edge (inferred, not seen): a ray that starts less than 0.01 inside a floor because of float noise could take that slab's exit face as a floor. That only matters if parry reports the exit normal as facing up.

## Teamkills (gauntlet 608 / 624, all_dials_on): does not reproduce on Linux ⏳
| Commit | capture_the_flag | zombie_survival |
|---|---|---|
| 4571b9c (before navigation) | pass | pass |
| 5b4fc664 (navigation merge) | pass | pass |
| ba54174f (after ally harm) | pass | pass |
| e05a1383 (head), release | pass | pass |
| e05a1383 (head), **debug** like the PC gate | pass | pass |

- Seed 0 at the head: CTF 26 kills, 0 team kills; zombie survival 33 kills, 0 team kills.
- **Other seeds can't be used for these scenes.** `tuning::with_seed(n > 0)` joins `n` players before the arena installs its packages, so `install_packages` panics with "Packages are enabled before players join" (gauntlet/mod.rs:107-113 and :182). That is a test-harness bug: seeded runs of any package scene fail. Fix it in v0.2.7 so seeds can look for a repro on Linux.
- With no Linux repro, the bisect has to run on the PC. Run `cargo test -p bri-chaos --test bot_gauntlet -- --exact capture_the_flag zombie_survival` at 4571b9c, 5b4fc664, ba54174f, then `git bisect` inside whichever merge goes red. The release lane reports that each branch passes alone and only the merged stack fails, so bisect the second parent's commits on top of the first parent.
- **Likely interaction (inferred, not verified):**
  - Ally harm prices teammates from where they are and where they are heading over `MOTION_AHEAD` (`Bodies::of` motions).
  - Navigation adds moves whose path that prediction doesn't model: leap legs (stand still, then jump), and the stall hop judged from `Brain::footing`.
  - A teammate that starts a leap or hop into the line between the shot check and impact would be the shape of "passes alone, fails merged".
  - The fix direction is to put a body on an air leg into the ally motion prediction with its planned arc, not to special-case it.
- Teamkills are in `tools/gate-known-failures.toml` (11eadefa); the v0.2.7 thread removes the entries when it fixes this.
