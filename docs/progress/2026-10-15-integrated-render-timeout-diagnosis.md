# 2026-10-15 Integrated rendering timeout diagnosis

Root reran the two old timed-out binaries from exact candidate
`ab10374f4a593e7eeb9fc1c4d3365046936c5bd8` sequentially while holding the real
gate lock. No gate checkout or binary was changed. Other development activity
continued, so this isolates gate test binaries, not the entire machine.

- `app_soak-de57076eb31beaa1.exe --include-ignored`: all five cases pass,
  277.16 seconds including process overhead.
- `app_item_render-d6bd5be06fac1f2e.exe --include-ignored`: all six cases pass,
  336.77 seconds including process overhead.

Evidence: `artifacts/gate-timeout-isolated-diagnostic.log`. The diagnostic lock
was released after both completed. Its 1,200-second per-binary watchdog did not
change test assertions or constitute a gate pass. The old full gate remains
failed.

The old full gate also explicitly reported that the formerly allowed
`steady_play_builds_and_uploads_nothing_whole::content` case now passes. The
sequential diagnostic confirms that result, so root removed that stale failure
allowance. The remaining two vehicle/portal failures retain their existing
entries and are still root-owned v0.3.1 release corrections; their earlier
diagnostic descriptions do not prove the current implementation correct.

`python -m unittest discover -s tools -p test_gate.py`: all 43 gate tool tests
pass after the allowance removal; `artifacts/gate-allowance-removal-tests.log`.

For the next combined candidate, root will use eight test jobs and a
20-minute binary watchdog, without removing any test, changing deterministic
work budgets or performance assertions. Three heavy binaries can share the
GPU in the gate, unlike this sequential diagnostic. A longer process watchdog
allows the measured workloads to finish under that contention and does not
waive any named failure. The candidate must still pass its own complete gate
and exact-head Windows checks before main integration.

Public integration PR: <https://github.com/MaxHastings/BlocklandReImagined/pull/172>.
It currently contains committed shared mechanisms, the full fidelity release
hold and root's bounded crash-cleanup observation correction. The next atomic
image-origin commit is still pending its agreed focused checks. No public main
push or publication has occurred.
