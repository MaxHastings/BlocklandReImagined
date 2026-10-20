> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-crash-snapshot-wait.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: crash-capture test, wait for the snapshot clone to go

Branch origin/claude/fix-crash-snapshot-wait = **3db2d466** (test only, on claude/v026-stack-plain c514ff4e).
Windows-only code; I read it in the cloud and couldn't run it (no Windows here).

## Verdict: PASS with one comment fix (no code change). Merge the sha that carries it. Not a v0.2.6 blocker; the test-strength gap goes to v0.2.7.

## 1. Is 3db2d466 sound, and not weaker than before?
It is sound, and it isn't weaker.
- The old assert checked `children(child)` once, right after the report appeared. Freeing the snapshot (or the process ending) starts the clone's teardown, but Windows removes the process asynchronously. That one look raced the teardown; on a loaded box it failed 2 of 12.
- The new loop waits up to `NATIVE_CAPTURE_WAIT` for the clone to go. It fails if the clone is still listed once the crashed process has exited, or if the clone never goes. Every failure the old check could catch, the new one still catches, without the race.

## 2. What the lane's finding means for 1d137e9a / 0a76149b
The finding: with both `free_snapshot()` calls commented out, the old and new tests still pass. So **neither test has ever shown that the free works.** The must-fix "test no clone left behind" from the 1d137e9a review was met in name only. That's my miss as well as the lane's.

What ends the clone without the free (inferred from the code; please verify on the PC):
- `on_native_crash` returns `EXCEPTION_CONTINUE_SEARCH`, so after the report the process goes on to WER or to termination.
- When a process terminates, its handle table is closed when its last thread exits, and that closes the snapshot handle, which ends the clone. The process object is signalled only after that.
- So for a moment the clone is gone while `try_wait()` still says the crashed process is running. That's exactly what the test sees with the frees removed.
- On the test machine, WER evidently doesn't hold the crashed process for long (no consent dialog in a test run), so it exits right after the report.
- A second possibility: WER's own process reflection makes a clone with the same image name and parent, which the `children` listing can't tell from ours.

Is the leak that 1d137e9a fixes real? **Yes, in principle (inferred).** On a player's machine with WER's default UI ("has stopped working"), WER keeps the crashed process alive until the dialog closes. Its handles stay open, so without the free the clone (a full copy of the game's memory) stays alive just as long. It only happens when the dump stalls past `DUMP_WAIT` (10 s), which the snapshot change made rare. So the fix is right and worth keeping. It just isn't proven by a test.

## 3. Does any of it block v0.2.6?
No.
- **Merge 3db2d466** (with the comment fix below). The old check is a flake that turns the gate red, and Max's rule is to fix the cause, which this does.
- **The leak fix stays as reviewed.** Its code is right; only its proof is missing. The case it covers needs a stalled dump AND WER holding the process.
- **v0.2.7, test strength:** make the test prove the free. Add a test-only hold (like `STALL_DUMP_VAR`) that keeps the crashed process alive after its report, and assert the clone goes while it's held; that fails with the frees removed. Better still, out-of-process dumps (already on the v0.2.7 list) remove the clone entirely, and this test becomes "no clone ever".

## Must-fix (comment only)
The new comment says the loop guards against "a clone left to go only when that process does". It doesn't: it passes with the frees removed. Reword it to say what it checks, e.g. "the clone must not outlive the crashed process. Whether the free or the process's own exit ends it isn't told apart here (v0.2.7: hold the process after its report)." No claims ahead of the code.

## Addendum (22:07Z): timing from the capabilities lane
Over 3 runs without the frees and 4 with them, the clone vanished 1.4 to 1.9 µs from the crashed process's exit. So the clone ends at process exit even **with** the frees: `PssFreeSnapshot` does not end it promptly today. The comment fix is 633c2a03.

Verdict unchanged, still not a v0.2.6 blocker:
- The clone is a copy-on-write VA clone of a process that is frozen while WER holds it, so its real memory cost is small.
- It goes when the crashed process goes.
- No player-facing behaviour depends on the free.

**v0.2.7 item (updated):** the free doesn't end the clone. Find what does (closing the clone's process handle? a `PssQuerySnapshot` of the clone's process plus `TerminateProcess`? a different flag set?), or skip that and move to out-of-process dumps, which removes the clone and this question. Then reconsider the timeout free: if it doesn't end the clone, it only adds the "freed under the dump thread" race that 0a76149b's comment describes. Test it by holding the crashed process open after its report and asserting the clone goes while it's held.
