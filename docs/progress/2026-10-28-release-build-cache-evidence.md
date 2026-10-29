# 2026-10-28 Release build cache retention and diagnostics

Separate follow-up to the released v0.4.0 and the v0.4.1 integration. This
branch changes build tooling only. It is not a prerequisite for v0.4.1.

## Evidence and limits

GitHub release run 40790021039 compiled the native shipping executables in
48m19s on Windows, 49m49s on macOS and 20m26s on Linux. The previous published
v0.3.7 run 40273672076 took 54m50s, 38m31s and 22m20s respectively. These are
Cargo's main release-build durations, not total packaging/job durations.
All six dependency-cache restores reported no cache found. Linux v0.4.0
sccache counters reported zero hits and 463 misses. Windows/macOS final
counters reported only 6/7 misses despite hundreds of compilation lines;
they cannot be treated as whole-build totals. Daemon idle resets are a
plausible explanation: releases omitted the persistent-daemon setting
already used by CI.

The cache inventory occupied 9.99 GiB across 2,589 entries. PR-only caches
held roughly 2.55 GiB; the older Windows/macOS release dependency caches
were absent. This establishes cache pressure and repeated cold dependency
builds. It does not establish how much of the platform gap is memory,
codegen, linking or filesystem overhead. The logs' long quiet tails do not
measure individual crate/linker duration. Raw logs and the cache snapshot
are retained in ignored artifacts/future-release-builds/.

GitHub's public macos-14 runner has three M1 cores/7 GB, compared with four
cores/16 GB for the Linux and Windows runners. That is another plausible
constraint, not a measured explanation for the entire difference.

Sources: [runner hardware](https://docs.github.com/en/actions/reference/runners/github-hosted-runners),
[cache scope/eviction](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching),
[rust-cache cleanup](https://github.com/Swatinem/rust-cache),
[sccache Rust limits](https://github.com/mozilla/sccache/blob/main/docs/Rust.md),
[GHA read-only cache mode](https://github.com/mozilla/sccache/blob/v0.19.0/docs/GHA.md),
[Cargo timings](https://doc.rust-lang.org/cargo/reference/timings.html).

## Changes

PR CI restores caches but no longer writes caches scoped only to that PR:
both rust-cache's save-if and sccache's GHA read-only mode enforce this.
Push/nightly/manual default-branch runs continue to populate reusable caches.
Tradeoff: rerunning a PR whose changed dependencies are absent from main can
recompile them; unrelated PRs/releases retain more useful shared entries.
No existing caches are deleted and no quota or paid runner changes are made.
Release dependency caches also save on failure.

All three release workflows keep the sccache daemon alive through long final
links. A small standard-library wrapper runs the original Cargo artifact set
with the additional diagnostic --timings flag, samples host memory every ten
seconds, and retains elapsed time, exit status and cache counters. Evidence is
uploaded for seven days even after build failure; it is outside player ZIPs.

Thin LTO, optimization level, static Windows CRT, symbols, target sets, Cargo
job scheduling and game code are unchanged. Experimental staged scheduling
was archived under ignored artifacts and is not shipped. No measured speedup
is claimed: compare subsequent native cold/warm runs before proposing a
scheduling/linker change. Diagnostic HTML should identify the expensive
crate/codegen phases; memory samples can show pressure/compression/swap.

## Verification

- Four focused Python contracts pass: platform executable sets, original Cargo
  failure status, missing Cargo failure reporting, and successful builds despite
  unavailable host observations without mutating environment variables.
- The observation fixture initially raced file creation; it now waits for both
  existence and a flushed sample before reading. This was a test-fixture error.
- All workflow YAML parses; maintained documents and history/protocol checks
  are run before push. Platform CI remains required before merge.
- No native release build, GPU suite, full gate or workflow dispatch was started
  for this tooling change. The v0.4.1 coordinator owns the current release.

Next: review/merge separately, then inspect actual native release evidence and
cache reuse before claiming reduced wall time. Preserve runtime performance as
an acceptance constraint for any further scheduling or linker experiments.
