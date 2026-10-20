> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-crash-capture-hang.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Design review: crash capture hang, snapshot dump (Windows, crates/crash/src/windows.rs)

Read against windows.rs at ab8f7f02 (the version 3b89a37f reverts to). The design note is the coordinator's relay of the PC lane's note; no code yet.

Verdict: PASS on the design, with four must-fixes for the build.

## Diagnosis: sound
- The hung stacks fit the mechanism. An in-process MiniDumpWriteDump suspends every thread but the caller, including the crashing thread in recv_timeout, so DUMP_WAIT can never fire. Any suspended thread holding the loader lock or a heap lock then blocks the dump forever.
- HeapLock made it worse: it took the heap lock first and then needed the loader lock (dbghelp's lazy LoadLibrary). That is the opposite order to the parallel-load workers, so the hang went from about 1 in 10 to 8 in 10.

## Q1: is the snapshot approach sound from an unhandled-exception filter? Yes.
- PssCaptureSnapshot with PSS_CAPTURE_VA_CLONE, then MiniDumpWriteDump on the snapshot handle with a callback answering IsProcessSnapshotCallback (S_FALSE), is Microsoft's documented pattern for dumping your own process.
- The capture is done by the kernel, and nothing in the live process is suspended. So every lock holder (workers, the loader) runs to completion, and the crashing thread's recv_timeout(DUMP_WAIT) is a real bound again.
- The dump still runs on the bri-dump thread, not the crashing one. Good.
- If the dump thread blocks on a lock the CRASHING thread holds (a crash inside DllMain or inside the heap), the crashing thread still wakes at DUMP_WAIT and writes the report. No hang; the report just says the dump failed.

### Must-fixes for the build
1. **No live-dump fallback anywhere.** dump() today falls back to `write_minidump(...)` on the crashing thread when there's no dumper (windows.rs ~173). That path and any "snapshot failed" path must not call MiniDumpWriteDump on the live process. When the snapshot fails (e.g. a VA clone of a large game process can fail for lack of commit), write the report with "minidump failed: <reason>" and stop.
2. **Free the snapshot on every path.** PssFreeSnapshot in a guard, so the VA clone, which is a whole cloned process, never outlives the dump. That covers errors and DUMP_WAIT timeouts. Put the "minidump failed" reason in the report, not a panic.
3. **Capture what the dump needs to be useful.** Include PSS_CAPTURE_THREADS | PSS_CAPTURE_THREAD_CONTEXT with full context flags, and keep MINIDUMP_EXCEPTION_INFORMATION (ThreadId = crashing thread, ClientPointers = 0; the pointers are in this process). Otherwise the dump has memory but no crash context. The test should check the .dmp has the exception stream (e.g. read the header's stream directory), not only that the file exists.
4. **Feature flag only.** Add Win32_System_Diagnostics_ProcessSnapshotting to bri-crash; the lockfile stays unchanged, as the note says. Confirm a Windows cross check (x86_64-pc-windows-msvc) and clippy -D warnings, since only the PC and the Windows CI build this file.

## Q2: is a test-only LdrLockLoaderLock acceptable and hermetic? Yes, with two conditions.
- It's an undocumented but long-stable ntdll export. It runs only inside the crash test's own child process, so it's hermetic: nothing outside the child is touched.
- Conditions:
  (a) The holder releases when the .dmp file EXISTS. CreateFileW makes the file before MiniDumpWriteDump runs, so the fixed path proceeds and the old path deterministically can't, because the holder is suspended before it sees the file.
  (b) The parent test kills the child at a cap and fails with "hung", so the "before" run proves the hang without hanging the gate.
- Show both runs (old: hangs at the cap every time; new: report says "minidump: ...").

## Q3: does anything still allocate or take the loader lock on the crashing thread before DUMP_WAIT? Yes. It's bounded now, but list it.
- Before the wait, on_native_crash allocates: format!, crate::unique, the timestamp string, path.to_path_buf, the mpsc channel. It also takes two Rust mutexes (state.writing, DUMPER). None of these takes the loader lock.
- With nothing suspended, a heap-lock wait here only waits on a running thread, so it finishes. That's no longer a hang.
- What's left (accepted for v0.2.6): a crash caused by heap corruption can fault again inside these allocations. Then Windows ends the process with no report, but it doesn't hang.
- The full answer is still the v0.2.7 out-of-process dump (already listed). Optional now: build the dump path and channel in advance (at install or in the dump thread), so the filter only fills a preallocated slot and signals.

## Tests the lane listed
- 20 release runs with a 30 s cap each, plus the deterministic loader-lock test: good.
- Add the exception-stream check (must-fix 3).
- Run the debug capture test too, because the original hang was in debug.

## Build review: cacd4e2e (claude/fix-crash-capture-hang, one commit on main fd719c94)
Verdict: PASS with one must-fix (the clone must not outlive a timed-out crash). The four design must-fixes are met.

Checked here: `cargo clippy -p bri-crash --tests --target x86_64-pc-windows-msvc -D warnings` is clean (cloud cross-check, no run).

### The four must-fixes from the design round
1. **No live-dump fallback: met.** dump() now errors with "the dump thread is not running" instead of dumping on the crashing thread. A failed snapshot reports "minidump failed: the process snapshot failed: ...". MiniDumpWriteDump only ever gets the snapshot handle.
2. **Snapshot freed: met on every path inside the dump thread.** `Snapshot` frees on Drop, before the result is sent. See the must-fix below for the one path it can't cover.
3. **Crash context: met.** THREADS | THREAD_CONTEXT with CONTEXT_ALL per arch, and MINIDUMP_EXCEPTION_INFORMATION kept (crashing ThreadId, ClientPointers 0). Both native tests parse the .dmp (MDMP header, exception stream, thread list) and check 0xC0000005 on the thread the child names. The callback reads and writes the packed structs unaligned, which is correct.
4. **Feature only, Windows check: met.** One feature line in Cargo.toml, lockfile unchanged, clippy clean on the Windows target.

### Must-fix: a crash that hits DUMP_WAIT leaves the VA clone running
- The VA clone is a separate (suspended) process holding a copy-on-write image of the game. PssFreeSnapshot is what ends it.
- If DUMP_WAIT expires, the crashing thread writes the report and the process exits while the dump thread still owns the snapshot. Drop never runs, so nothing ends the clone.
- PSS_CREATE_BREAKAWAY makes this worse: the clone leaves the parent's job, so a launcher or the test harness killing the job (or the test's `child.kill()` at NATIVE_CAPTURE_WAIT) doesn't take it down either.
- Result: a leftover process holding the crashed game's memory until reboot. Inferred from the PSS docs as I know them; please verify on the PC by forcing the timeout (e.g. a test-only DUMP_WAIT of zero) and checking the process list.
- Fix, no new constant: after capture, query the clone (PssQuerySnapshot, PSS_QUERY_VA_CLONE_INFORMATION) and keep its handle where the filter can reach it. On the timeout path the crashing thread terminates the clone before it returns.
- Also: say in the code why BREAKAWAY is there. The error 87 was very likely BREAKAWAY_OPTIONAL used without BREAKAWAY. If breakaway isn't needed for a reason you can name, drop both flags so job teardown also cleans up.

### Is the swapped test a fair proof of the original hang? Yes.
- It recreates the field mechanism the stack walk showed: loader and heap busy on other threads (load/unload plus thread starts) while main crashes. Old windows.rs: 8 of 10 failed (6 hung). Snapshot: 60 of 60.
- It's probabilistic rather than deterministic. But it detects the regression strongly (80%), and a fixed build passed 60 in a row, so it's not a flake risk as written.
- Dropping the loader-lock-holder test is fine since it didn't reproduce. Say in the progress note why it didn't hang, because that's evidence about which lock really mattered: the heap/loader inversion, not the loader lock alone.

### BREAKAWAY side effects
- Escaping the job, as above, is the only one that matters here. The rest of the flags (USE_VM_ALLOCATIONS, RELEASE_SECTION) are standard for dumping.

## Delta 1d137e9a (on cacd4e2e): PASS. Merge 1d137e9a08d1f1325b263a457cab3c3a4b3c41d9.
Checked here: clippy -D warnings on bri-crash --tests for x86_64-pc-windows-msvc is clean (cross-check, no run).

- The clone must-fix is fixed. Without breakaway the clone stays in the job and ends with the process (the lane's PC finding). While WER keeps the crashed process alive, the timeout path now frees it. The new test a_dump_that_never_finishes_leaves_no_snapshot_behind failed 3/3 without the timeout free.
- Breakaway: both flags dropped, and error 87 is explained (OPTIONAL without BREAKAWAY). Good.

### Swap/free race: no double free.
`SNAPSHOT.swap(0, AcqRel)` hands the handle to exactly one of the two threads. If the timeout fires before capture finishes, the crashing thread finds 0 and the dump thread's Drop frees it later. That's correct.
Known tradeoff, not blocking: on the timeout path the crashing thread frees the snapshot while dbghelp may still be reading it on the dump thread. Worst case the dump thread faults. Its filter then waits on `state.writing`, which the crashing thread holds while it writes the report, and the process ends through WER after that. So it's bounded, and at most a spurious second report. This path only exists after a 10 s stall; the v0.2.7 out-of-process dump removes it. Say so in a comment at free_snapshot's timeout call.

### Env-var test hook in the shipped crash path: acceptable.
It's read only at crash time, on the dump thread after capture, and can only stop a dump (the report is still written after DUMP_WAIT). Integration tests link the normal library, so cfg(test) can't hide it.
Optional tidy (v0.2.7 or now): a `test-hooks` cargo feature that the crate's own dev-dependency turns on. Resolver 2 keeps dev-only features out of release builds, so the hook isn't compiled into players' builds at all.

### Nothing else blocking
The Toolhelp feature is dev-only for Windows, and the lockfile is unchanged. Release capture 20/20 and busy-loader 20/20 are the lane's numbers.
