> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-stand-up-camera.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: first-person stand-up camera (PR #26), head e2048f2, base main a78b3a5

This was an independent, read-only review. Nothing was built or run. The diff is 4 files and about 110 lines, client only.

## Verdict: passes. No blockers.

It fixes the cause, not the symptom:
- While seated, the first-person eye followed the drawn body's `Eye` node.
- The moment `sitting` cleared, `posed_eye` returned `None` and the camera jumped to the standing eye, while the body was still drawn half seated.

Now, for the length of the body's own blend out of `sit`, the eye eases from the drawn head to the smoothed standing eye (`crates/client/src/app/view.rs:217-226`). Both ends match what the player saw before and after, so nothing can jump.

## Checked against the ask

**No new timer or constant.**
- The ease uses the existing action-transition time, `TRANSITION_TIME` (0.25 s, v20's `sAnimationTransitionTime`).
- `stood_at` is a timestamp of when the body left the pose, not a countdown (`crates/client/src/avatar.rs:780, 1386-1390`).
- `getting_up()` derives the remaining share from the animation clock (`avatar.rs:1067-1073`).

**No special case.**
- `SIT` names the existing `sit` sequence, which was already the literal there (`avatar.rs:969, 1350`).
- The ease reads the same body pose that sitting already follows.

**State hygiene.**
- `stood_at` is carried across an outfit rebuild (`avatar.rs:1692`).
- It is cleared on respawn (`avatar.rs:1709`) and on sitting back down (`avatar.rs:1386`).

**Consumers.**
- `mounts.posed_eye` feeds only the first-person eye (`app/view.rs:162`, `app/render.rs:559-562`).
- Nothing reads `Some` as "seated".

**Test.**
- `getting_up_eases_out_of_the_sit_over_one_transition` (`avatar.rs:2955-2993`) checks that:
  - it's `None` while sitting;
  - it's 1 at the moment of standing, with the eye unchanged;
  - it keeps its own clock when a run cuts in at half way (0.5);
  - it's `None` after one transition;
  - sitting back down part way up isn't counted as getting up.
- It can fail for real, so it doesn't pass by construction.

## Not blocking (v0.2.7 notes)

- **Also applies to seats.** The same ease now applies when leaving any seat or mount point posed `sit`, because the body leaves `SIT` there too. Dismounting used to snap and will now ease from the drawn head over 0.25 s. That's probably an improvement, but it's untested and not mentioned in the write-up.
- **Dying while sitting.** The body leaves `SIT` for `death1`, so for 0.25 s the first-person eye eases from the dying head. It's harmless, and death normally switches the camera anyway.
- **Not seen in the game yet.** It hasn't been checked visually, as the write-up says. Max's playtest confirms it.
- **Real content not tested.** Only the synthetic avatar test ran. The content variant needs generated v20 content.
