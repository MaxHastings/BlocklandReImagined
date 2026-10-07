# 2026-10-07 Crash dumps from a process snapshot

Two hangs in native crash capture on Windows (`crates/crash/src/windows.rs`).

1. Release, with 0ff3381 (since reverted in 3b89a37f): the release capture
   test hung 5 of 6 times on Max's PC. Walking every thread of a hung child
   from outside (dbghelp `StackWalk64`): the dump thread held the process
   heap lock (`HeapLock`, added by 0ff3381) and, inside `MiniDumpWriteDump`,
   waited in `LoadLibraryExA` for the loader; a loader threadpool worker
   (`LdrResolveDelayLoadedAPI`) held the loader's work and waited for the
   heap lock; the crashing thread waited for the heap lock too, so its
   `DUMP_WAIT` never ran. A lock-order inversion that 0ff3381 introduced.
2. The original, rarer hang 0ff3381 aimed at: `MiniDumpWriteDump` on the
   live process suspends every other thread, then loads a library. A thread
   suspended in the middle of a library load or unload (or a thread
   start-up) never finishes it, so the dump's own load waits forever, and
   the crashing thread, suspended too, can never give up. Reproduced: a crash
   while two threads load and unload system libraries and start threads hung
   10 of 10 runs; the walked stacks showed every thread suspended once, two
   of them inside `LoadLibraryExW` / `FreeLibrary`, the dump thread waiting
   in `LdrLoadDll`.

## Fix

The dump thread captures a snapshot of the process (`PssCaptureSnapshot`: a
clone of the address space and every thread's full context, taken by the
kernel) and writes the minidump from it, answering dbghelp's
`IsProcessSnapshotCallback`. Nothing in the process is suspended, so a
thread busy in the loader or the heap finishes and the dump goes on, and
the crashing thread's `DUMP_WAIT` always ends. The snapshot is freed on
every path (`Snapshot`'s drop). It never falls back to dumping the live
process: a failed snapshot, or a crash before the dump thread runs, writes
the report with the reason.

Known leftover, accepted in review: the crash handler still allocates before
its wait, so a crash from a corrupted heap may end with no report. An
out-of-process dump is v0.2.7 work.

## Evidence

- `a_native_crash_while_other_threads_load_libraries_still_dumps` (new):
  with the live-process dump it failed 8 of 10 release runs (6 killed as
  hung at `NATIVE_CAPTURE_WAIT`, the capture's own longest wait); with the
  snapshot 60 of 60 passed.
- `a_native_crash_leaves_a_minidump_and_a_report`: 20 of 20 release runs,
  four times, one test thread, 30 s cap each. Both tests now parse the dump: its
  exception stream holds the access violation on the crashing thread, and
  its thread list holds that thread.
- The children are killed past `bri_crash::NATIVE_CAPTURE_WAIT`, so a hang
  fails the test instead of hanging the gate.

A trap met on the way: restoring a source file with `Copy-Item` keeps its
old modification time, so cargo kept a stale build and a run measured the
old code. Builds compared here were checked by a string only the new code
holds.
