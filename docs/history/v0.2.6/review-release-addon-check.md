> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-release-addon-check.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: release zip check fails on left-out Add-Ons (8b066d83)

Branch claude/release-addon-check, one commit 8b066d8311734850551e121705d2e893c6c4908e on main fd719c94, .github/workflows/release.yml only.

Verdict: PASS with one one-line must-fix (an empty or swallowed output must not pass). Merge after that line.

## Can it pass silently?
- **--check crashes or errors:** fails. The exit code is kept (`$code = $LASTEXITCODE`, `exit $code`) before the match, and a native crash exits non-zero.
- **Empty output: PASSES today. Must-fix.** If output capture breaks, or --check returns 0 without printing (a future early return), there's nothing to match and the step passes. Fix: also require the line --check prints only on success, "Startup validation passed:" (client main.rs:124), and fail with "--check printed no result" if it's missing. One line, and it turns "no evidence" into a failure.
- **Wording:** robust for the case that matters. Every left-out Add-On reaches the player through one place, `notify_left_out_add_ons` (client app/addons.rs:538: "Add-On left out: {id}: {reason}"), called from the load with the whole left-out list (addons.rs:250). That covers both the one-Add-On case and the every-Add-On case. content.rs's own "Add-On {id} left out:" eprintln doesn't match, but it's always followed by the matching line. The match is SimpleMatch and case-sensitive. A rewording of addons.rs:538 would silently disarm it, so pin the prefix: a client test asserting the exact text, or use the structured report below.
- **pwsh `2>&1`:** with GitHub's pwsh (ErrorActionPreference=Stop), native stderr through 2>&1 doesn't terminate in PowerShell 7.x, and the merger's run on both previews shows the capture works. Keep it as is.

## Would a failing --check exit code be cleaner? Yes, but via the structured report, not by making --check strict everywhere.
- --check already collects left-out Add-Ons as health problems (`left_out_problem`, addons.rs:468) and writes `<state>/logs/add-on-health.json`. main.rs:154 says "The release gate reads the full list from logs/add-on-health.json", but nothing does yet.
- Making plain --check fail on any left-out Add-On would also fail the push gate whenever Max's own drop folder (content/Add-Ons) holds a broken Add-On. Wrong place.
- Cleanest: the workflow reads add-on-health.json from bri-zip-state/logs and fails on any left-out entry (or `--check --strict` for shipped content). That doesn't depend on wording and is the documented source. The string match is fine for v0.2.6 with the success-line must-fix; switch to the report in v0.2.7.

## Round 2: 95e9d469 (report-based, on 8b066d83). PASS. Merge 95e9d469876d813534371013d3adac77c7411429.
- Fails on: non-zero exit, missing report, schema_version != 1, any kind "left_out". The state folder is wiped first, so an old report can't pass. That closes the empty-output hole, and it no longer depends on wording. The merger's four runs (fd719c9 fails with 81, bfdf854 passes, silent exe fails, exit-1 exe fails) cover each branch.

### (a) Off-by-default bundles: a real gap, but narrow for what this check is for. v0.2.7.
- packages/default-addons.json carries 53 entries `"enabled": false` (Ragdoll, Gravity Gun, CTF and the bots among them). The report covers enabled Add-Ons only, so a broken off-by-default one ships silently.
- The failure this step exists for, a bundle packed against an older schema, is bundle-wide: tools/addon_bundle.py imports and packs every original in one go. Two originals are on by default (Duplicator, Stunt Plane), so a stale bundle still fails the step. That's inferred from how the bundle is built.
- What it misses: one off-by-default Add-On broken on its own (a port bug).
- v0.2.7: a second --check in the smoke over a copy of the content whose packages.json lists every default-addons.json entry in order. Or a `--check` option that loads each bundled Add-On, so every shipped Add-On is proven to load.

### (b) Session log can't reach the zip: confirmed.
- "Package" builds dist\BlocklandReImagined-windows.zip before "Zip smoke".
- The smoke expands it into $RUNNER_TEMP\unzipped and runs from that copy; its state is in $RUNNER_TEMP\bri-zip-state.
- "Gather release files" copies the original dist zip.
- Anything --check writes lands in the temp copy or the temp state, never in the shipped zip.
