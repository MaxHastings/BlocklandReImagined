> **Historical note. Written 2026-10-11 (the file's date in the cloud project folder, `v029-addon-compat/report.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# v0.2.9 Add-On compatibility report

PR #41 (draft, not merged): https://github.com/MaxHastings/BlocklandReImagined/pull/41
Branch `claude/v029-addon-compat-1khpmi`. Full detail in `docs/progress/2026-10-11-v029-addon-compat.md` on the branch.

## Script math (`ceil (f64)`)
- **Verified:** the v0.2.8 error reproduced exactly in a test. The cause was Rhai: its float math takes only floats and calls it `ceiling`.
- **Changed:** every float function now takes whole numbers. Rounding a whole number returns it unchanged. `ceil`, `trunc`, `fract`, `pow`, `log10`, `log2` and `clamp` were added. A call to a function that doesn't exist now warns when the Add-On loads, with the line number and a "did you mean".
- **Tests:** math and lint unit tests, plus a hosted-game test of the Rallypack shot line and of the unknown-call warning. All 28 repo scripts are free of false warnings.
- **Uncertain:** nothing known.

## Old-save prints
- **Verified:** v20 writes `NOPRINT` for "no print", and the printer refused any print it couldn't resolve. Nothing relied on that refusal.
- **Changed:** NOPRINT now counts as no print. The printer opens on any brick, with nothing highlighted when it doesn't recognise the print.
- **Tests:** the reader, plus the printer on five old-save spellings.

## "Rule owner may not move this object" (soccer)
- **Verified:** this was a wrong rejection. It used the physics-gun rule, which needs the builder alive, instead of the documented spawn-brick rule. The crash itself is the Soccer crash thread's.
- **Changed:** goal pushes and resets follow the spawn-brick rule.
- **Tests:** a goal pushes while its builder is dead, and a loaded goal pushes for an admin trusted with its events.

## Add-On models (P90)
- **Verified:** the P90's materials are flagged additive without the translucent flag, and Torque draws those opaque. None of the 154 v20 shapes has that flag mix. The stock item pack held only 6 of the 25 v20 shape textures.
- **Changed:** those materials now import opaque. The stock item pack carries all 25 textures, so `red`, `black25`, `yellow` and `gray50` resolve.
- **Uncertain:** that v20 drew Add-On parts from `base/data/shapes` is inferred, not measured. The importer already assumed it.

## Import tooling
- **Changed:** `import-report.json` and `IMPORT-REPORT.md` now give each pickable item a coverage level with reasons: preserved, ported, approximate, unsupported or missing source. The Rallypack/P90 recipes are now in Git, byte-identical, with your authorship. Your branch and worktree are untouched.
- **Not done yet:** dependency closure, and one build/check/package/install command.

## Release blockers (this area)
1. PR #41 CI green (the integration thread's automated gate decides the release).
2. Content rebuild and real-file P90 import on Max's PC, run headless by Claude through Remote Control, with results in the PR body.
3. Open: Butterfly Knife warnings (not investigated); bots' `Empty slot` (bots thread).

## Verified on Max's PC (headless, done)
- The content rebuild passed, and `bri-client --check` passed. The item pack now has all 25 stock shape textures.
- Both recipes rebuilt with no unresolved materials. The 6 private Rallypack/P90 tests pass.
- From the real P90 ZIP, 7 of 8 materials now import opaque. `red` stays additive because it sets translucent too, which matches Torque.
- The script, rules, BLS and printer tests pass against real content. The v20 saves contain no NOPRINT, so the print fix rests on unit tests.
- Report: Rallypack items rate 6 preserved, 4 unsupported and 45 missing source. The 45 are icon files the Add-On doesn't ship.

Note: subagents ran as Explore agents; GPT-6.1 Sol isn't available here.
