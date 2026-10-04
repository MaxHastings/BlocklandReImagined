# 2026-10-02 Gate corpus coverage reporting

Following the five bug fixes, Max requested useful technical debt and integration
cleanup. The fixed-save gate printed `save corpus: ok` before admitting that
no private saves were available. A folder containing only some of the authored
corpus similarly gave an unqualified success. This is evidence-reporting debt:
passing libtest does not necessarily mean the whole private corpus was checked.

The save test now emits the checked and authored counts after successful
conversion and hosting. The gate classifies a failed run before looking at skip
or coverage messages, reports skipped runs as SKIPPED, and reports partial and
complete coverage with explicit counts. A successful older test without counts
says coverage was not reported. The existing policy allowing an unavailable or
partial private corpus remains unchanged; no test is newly skipped or waived.

Seven Python regression tests cover all of those outcomes, absent named tests,
and inconsistent coverage counts. They run as a cheap gate step and in Windows
CI. They need no generated content. The isolated worktree is
`../BlocklandReImagined-worktrees/gate-coverage`, created from origin/main because
the main checkout belongs to the active bot/vehicle task. No original content
was changed, no game window or gameplay input was used, and no build target was
created in this worktree.

Verification so far: `/opt/homebrew/bin/python3 tools/test_gate.py` passed seven
tests; the changed Rust test was formatted with rustfmt (skip_children=true);
`git diff --check` passed. An initial `python3` in this worktree selected Apple's
Python 3.9 and failed importing tomllib; the existing gate requires Python 3.11+.
Retried with Homebrew Python 3.14.6. An initial rustfmt check requested wrapping
one added report line; that formatting was applied. The full gate runs with
Homebrew first on PATH and with CARGO_PROFILE_DEV_DEBUG=0 and CARGO_INCREMENTAL=0,
leaving CARGO_TARGET_DIR unset.

Final verification: the full gate passed and pushed `cd08153c` to main.
The seven gate-reporting regressions passed, then workspace build (37 seconds),
clippy with warnings denied (9 seconds), content validation (3 seconds), and all
317 Rust test binaries (395 seconds). No retry was required. The real corpus
step printed `save corpus: SKIPPED: no saves folder; set BRI_SAVES to Maxwell's
saves folder`, with no preceding corpus-success claim. Total gate: 445 seconds.
Remote main was verified as `cd08153c4f74801e6f3c1f4c51f815e19541a5bb`.
Logs: `../.bri-gate/logs/cd08153c4f74.log` and the retained main checkout's
ignored `artifacts/gate-corpus-coverage.log`.

The original five-fix Windows CI passed its build and clippy and is running
its tests. The new cleanup's
[Windows CI](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/37071011376)
started; it remains pending. The private saves corpus remains unavailable on
this Mac. No full-alpha acceptance item was checked off.

Next useful cleanup: replace revisioned content-folder literals in test
fixtures with package-role lookup. Large Add-On reloads still have an owned
performance follow-up, and the private CI-content artifact needs a refresh
from the authoritative content sources (see the Mac setup entry). Neither is
silently treated as resolved here. The isolated worktree is removed after its
commits are verified on main; it has no target folder to clean.

