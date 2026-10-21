> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-ci-timeout.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: CI timeout fix, claude/fix-ci-timeout @ df1198cc (base main c753eada3)

Read-only review of the one commit (3 files, +63/-8). Nothing run locally.

## Verdict: passes. Merge once run 37597628320 is green.

That run was still going when I looked (check job on Tool tests, all four shards testing). The merge bar is the run finishing green with every shard under 120 minutes.

## What I checked

1. **Nothing is dropped.**
   - The old Test step was `gate.py --ci-test`. It ran every test binary from `cargo test --workspace --no-run` minus `[[ci_skip_target]]`. It never ran doc tests or `--include-ignored`, and the new code doesn't either. That's unchanged, not lost.
   - `shard()` takes every Nth label from the Kth over the sorted, non-skipped labels. So the 4 shards are disjoint and cover the set. Each shard builds the same workspace on the same OS, so all four see the same label list.
   - If two binaries ever shared a label, both would run in the shard owning it. They'd still run, never be skipped.
   - The unknown-`ci_skip_target` refusal still runs in every shard.
   - The GPU provisioning step and the env (dx12, dxc, `RUST_TEST_THREADS=1`) moved with the tests unchanged.
2. **Known failures handling is the same.** `ci_test` still ignores `[[failure]]` entries, so a known failure still fails CI. That's no worse than before, and still the open v0.2.4 note.
3. **A failing shard fails the workflow.**
   - A test failure or a failed `--no-run` build returns False, which exits 1 and fails the job.
   - `fail-fast: false` only lets the other shards finish so every failure gets reported.
   - There's no `continue-on-error`.
4. **The check job still gates fmt and clippy.** Format, tool tests (now including the new `test_gate.py` shard tests), Build and Clippy `-D warnings` are all unchanged. Its limit drops to 60 min, which build plus clippy fits (inferred; the run will show it).
5. **Shard parsing** refuses K ≥ N, N = 0 and non-numbers (argparse turns the ValueError into a usage error).

## Notes (not blocking)

- **Required checks.** If a branch ruleset requires the status check named `check`, the new `test (0..3)` jobs aren't required, so a PR with a red shard could still show as mergeable. The local PC gate stays the merge bar, so this isn't a blocker. If Max wants GitHub to enforce it, add the four `test (N)` checks to the ruleset. I can't read the rulesets from here.
- Shards are even by count, not time. If one shard ends up holding most of the slow GPU targets, it could still approach 120 min. The run's per-shard times will show it.
- Each shard compiles the full test build itself. That's expected; rust-cache helps after the first run.
- `--shard` without `--ci-test` is silently ignored. That's harmless.
