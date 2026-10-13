# 2026-10-12 Faster release pipeline

## Where v0.2.10's time went

The integration branch was ready at 00:46 UTC on 2026-10-12. The release
builds were still running at 02:50. Steps marked ✋ waited on Max.

| Step | Minutes | Kind |
|---|---|---|
| ✋ Paste to start the final gate (00:46 → 00:55) | 9 | process |
| PC finished perf runs I/J/K before the gate (→ about 01:23) | 27 | process |
| Final gate on 3ad5c21 (about 01:23 → 01:50) | 27 | build and test |
| Two test-setup failures: fix 9ca6cb5, ✋ paste, targeted rerun (→ 01:56) | 6 | process |
| ✋ Merge PR #86 waited on the GitHub Windows check until Max bypassed it (→ 02:11) | 15 | process; that run took 41 min (01:52 → 02:33) |
| PC content refresh and uploads, after the merge (02:12 → 02:36) | 24 | real work at the wrong time |
| ✋ Pastes for the tag, the builds and a local zip (02:14, 02:18) | 0 | process |
| Windows release build, cold (from 02:37; v0.2.9's took 25 min, 21 of them compiling) | about 25 | build |
| Mac and Linux, run after Windows (v0.2.9: 10 and 15 min, side by side) | about 15 | process |

That is about 50 minutes of work that has to happen (one gate, one release
build) and about 75 minutes of ordering and waiting. Max pasted 5 messages
and made one bypass call.

## What changed

- **Mac and Linux build alongside Windows.** They used to take the content
  out of the finished Windows zip, so they could only start after it. Now
  they fetch the same `ci-content` and `addon-bundle` drafts Windows does.
  `release.yml` runs all three at once, then one publish job
  (`tools/publish_release.py`). That job checks every zip's MANIFEST names the
  version and that the base content packs are byte-identical across the
  three. It writes SHA256SUMS, takes the notes from
  `docs/releases/<version>.md`, and publishes as latest. This replaces the
  hand-run "staging one cross-platform release" steps.
- **`tools/release.py`** runs the whole release on the PC from one command, in
  the right order:
  1. Content refresh on the release commit, uploading only what changed.
  2. Start the build-only release run, which goes on during the gate.
  3. The gate.
  4. Push the gated commit to main (the hook reuses the pass) and push the tag.
  5. Wait for publishing.

  Because nothing goes through a merge pull request, the duplicate GitHub
  Windows check never blocks a release. It still runs on main afterwards. The
  gate now tests the content that ships. On v0.2.10 it tested the old packs,
  and four were rebuilt after the merge.
- **`publish-release.yml`** publishes when a `v*` tag is pushed. It reuses the
  zips the build-only run made at that commit and waits for that run if it is
  still going. Only when there is no such build does it start one.
- **sccache** is in the three release builds, as in `ci.yml` since #83, so
  workspace crates compiled by one release run are reused by the next.

## Expected next release

| Step | Minutes |
|---|---|
| Content refresh and uploads | a few when no importer changed; up to about 24 |
| Gate (about 27) and the three release builds (about 25) side by side | about 27 |
| Push, tag, publish the prebuilt zips | about 3 |

That is about 35 to 55 minutes from Max's go-ahead, against about 2 h 35 min
for v0.2.10 from a ready branch, with one message from Max instead of five.
The gate and one release build are the floor. The sccache saving on the
release build is not measured yet. Read it from the next run's build step:
v0.2.9's Windows build step compiled for 21 min.

## What needs Max (one step each)

- **The go-ahead.** One message to the Claude on the PC:
  `python tools/release.py v0.2.11 --branch <integration branch>`.
  A cloud thread cannot push tags. The PC session's safety check needs Max's
  own words for publishing. So this one message stays, unless Max adds the
  allow rule below.
- **Optional, for zero pastes.** Add `"Bash(python tools/release.py *)"` to
  `permissions.allow` in the repository's `.claude/settings.json`. Allow
  rules apply before the auto-mode check. With it, a release thread's
  Remote Control session on the PC can run the command when Max says
  "release it" in the thread. A Claude session can't write this rule itself;
  that is blocked as self-modification, as in the "Full autonomy on the PC"
  thread.
- **Only if the push to main is refused.** If a main ruleset requires a pull
  request or status checks, add Max's account as a bypass actor: Settings,
  Rules, the main ruleset, Bypass list. `gate.py --push` already pushes to
  main this way, and Max had the bypass button on #86, so this is expected to
  be in place already.

## Not done, and why

- **A green PC gate satisfying the GitHub check on a pull request.** It isn't
  needed once releases push the gated commit directly. A pull request's
  check starts before the gate passes, and a status the PC posts later can't
  re-run it.
- **A warm cache for the tag build.** The tag no longer starts a build. The
  build runs during the gate, so its 25 minutes are hidden.

## Evidence

- Run timings come from the GitHub Actions API: release run 38132759197
  (v0.2.9), release run 38196485696 (v0.2.10), ci run 38193572955 (#86 head
  9ca6cb5).
- The other times come from the "Integrate and gate v0.2.10" thread.
- `python -m unittest discover -s tools -p "test_*.py"`: 89 tests OK,
  including the new `tools/test_publish_release.py`.
- actionlint is clean on all four workflows.
- The workflows have not been run end to end yet. The first real run is the
  next release, or a `--check-only` run before it.
