# 2026-10-28 Remove redundant crash test and flaky harness assumptions

Maxwell requested cleanup of the two crash-capture flakes reported by the
successful gameplay/audio gate at `3bea0b044`. The plain native-crash test
duplicated the loader-stress case's exact dump/report/exception-thread checks;
that duplicate is removed. The remaining snapshot cases run serially within
their fixture, use file-backed stderr and sleeping polls, and suppress Windows
error UI only in disposable children. A separate bounded subprocess budget
covers startup, file I/O, scheduling and teardown; the production 10-second dump
and two-second log-drain limits remain unchanged. The misleading public comment
describing their sum as a process-exit deadline is corrected.

Quick/focused checks for bri-crash pass: nine units, warning-free clippy and all
five remaining integration cases. Twelve complete fixture runs with eight test
threads, four concurrent processes, pass without retry (60 test executions,
about 10.1 seconds per run). Real access-violation dump validation, busy-loader
regression coverage, panic/copy-without-symbols checks and forced-timeout clone
cleanup remain. No ignored tests, gate exemptions or production behavior changes
were introduced. Maxwell then requested merging without further refinement;
the existing full implementation gate and these targeted checks accompany the
required Windows CI for the final PR head.
