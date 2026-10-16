# 2026-10-15 Observe bounded crash and snapshot cleanup

Root is integrating v0.3.1 on `codex/v031-main-integration`. The previous full
gate at `ab10374f` failed the stalled-dump capture test twice, then the same
cached candidate test passed alone. Production crash capture is unchanged
between the relevant baseline and that candidate.

The fixture sampled clone processes with Toolhelp and then queried parent
exit status. Both processes could exit between these separate observations,
making a stale process-list entry look like a clone surviving its parent.
Toolhelp provides a captured list, not an atomic observation with `try_wait`:
<https://learn.microsoft.com/en-us/windows/win32/api/tlhelp32/nf-tlhelp32-createtoolhelp32snapshot>.
The old comment acknowledged delayed teardown while the assertion required an
ordering those observations cannot establish.

The fixture now waits within the existing `NATIVE_CAPTURE_WAIT` cleanup bound
for both an unsuccessful parent exit and a fresh empty clone-process list.
Timeout diagnostics include the observed parent status and clone IDs. It still
requires the snapshot to appear, the failed-minidump report to be written and
the report to contain the original timeout message. It now also bounds the
parent exit, which previously ended with an unbounded `child.wait()`.

This establishes bounded cleanup, not simultaneous termination ordering. No
production lifetime change, early snapshot free, gate exemption, ignored test
or new timeout allowance was introduced. The dump worker continues to own its
snapshot; the earlier unsafe cross-thread free is not restored.

Validation on the primary checkout:

- `python tools/check.py quick --crates bri-crash`: format and nine unit tests
  pass; `artifacts/crash-cleanup-quick.log`.
- `python tools/check.py focused --crates bri-crash --depth 0`: warnings-denied
  all-target Clippy and all six capture integration cases pass, including native
  crash, busy loader, copied executable and stalled dump. Nine unit tests also
  pass in this run. Three steps complete in 24 seconds;
  `artifacts/crash-cleanup-focused.log`.

The full integrated gate remains pending the committed image-origin slice and
completion of the old cached rendering diagnostics under the gate lock. This
focused pass is not a full-gate result or release acceptance.
