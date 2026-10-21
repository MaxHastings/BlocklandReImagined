> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-crash-hang.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Crash-capture hang: 0ff3381 (claude/fix-linux-boot-c5rao2, on ee04184)

**Verdict: PASS. Merge 0ff3381788e95a0c4dc0d3bbedab692bd7ca2cd4.**

Windows `cargo check --tests` and `clippy -D warnings` for bri-crash
(x86_64-pc-windows-msvc) are clean here. windows-sys already enables
Win32_System_Memory. Not run on Windows: the PC gate's run of
a_native_crash_leaves_a_minidump_and_a_report is the proof.

## Cause: plausible
- In-process MiniDumpWriteDump is documented as unsafe when any suspended
  thread holds the loader lock or a heap lock it needs.
- A just-created thread runs DLL_THREAD_ATTACH and the static CRT's TLS
  callbacks under the loader lock before its body. The test crashes
  immediately after `install` spawns two threads, and load stretches that
  window.
- That fits "hangs only under heavy load, passes 32/32 in parallel".
- Inferred from code, not reproduced. Nobody has a Windows trace of the hang.

## Handshake (start_thread): sound
- A failed spawn returns Err before `recv`.
- The only code before `send` is the `send` itself, which can't panic.
- If the thread dies first, `recv` errors and is reported.
- `send` runs in the body, which Windows only enters after the attach
  notifications, so a return means the thread is past the loader lock.
- New wait: install now waits for the thread to be scheduled. It can only
  stall if another thread holds the loader lock for a long time, which is
  bounded by that thread. That is acceptable.

## HeapLock: safe, and DUMP_WAIT does end the case asked about
- Rust's System allocator and the static UCRT both allocate from
  GetProcessHeap, so one lock covers them.
- Other threads holding the lock finish their call and release it. Then they
  block waiting for it (not holding it) and get suspended there harmlessly.
- The lock is re-entrant, so dbghelp's own allocations on the dump thread
  work.
- **Crashing thread holds the heap lock** (a crash inside HeapAlloc/HeapFree):
  - The dump thread blocks in HeapLock *before* anything is suspended.
  - The crashing thread's 10 s wait expires and it writes the report.
  - Its own allocations re-enter the lock it holds, so they work.
  - The process exits. So DUMP_WAIT ends it, as the comment says.
- A failed HeapLock still dumps, unlocked. That is no worse than before.

## Slop
None. The DUMP_WAIT comment is now honest about what it bounds.

## Residual (v0.2.7, not this fix)
The handshake protects only the crash crate's own two threads. A crash in
the game while any *other* thread is starting (rayon, wgpu, audio) or holds
the loader lock (a LoadLibrary in flight) can still deadlock the in-process
dump with no timeout. The player sees a frozen window instead of a crash
report.

The industry-standard fix is an out-of-process dump: a small watcher process
(or the launcher) that calls MiniDumpWriteDump on the crashed process, as
Crashpad and Breakpad do. Rare, so not a blocker. Worth a v0.2.7 line.
