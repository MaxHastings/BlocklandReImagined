# 2026-10-23 Workshop render check resolves the active UI pack

The v0.3.6 candidate `021cffe082ec79ec76a231beb9ed2bde697b8433` completed
all 542 test binaries, but its full gate correctly refused integration:
`workshop_offscreen` tried to load `content/ui-pack-004/ui-pack.json` from
the clean public release content. The current role is `ui-pack-005`, and the
retained historical 004 folder in the primary checkout had masked this
assumption. This is a test-harness correction, not a Workshop/gameplay fix.

The test now resolves `ui_pack` with the same existing
`bri_package::testing::pack_dir` helper used by other current-pack checks.
Its rendering, resolutions, missing-texture assertions and captures remain
unchanged. No old pack is copied under a misleading name and no failure is
waived. The exact before log and complete failed gate are retained privately
as `workshop-active-pack-before.log` and `gate-021cffe082ec-full.log`.
The corrected offscreen test passes in 4.36 seconds; the affected focused
tier passes in 138 seconds, including strict clippy, UI/importer integrations
and all 666 client unit tests. Logs: `workshop-active-pack-after.log` and
`workshop-pack-focused.log`. The full gate, Windows CI and release build use
the resulting source pin.

The same gate classified `volume_hooks::entering_and_leaving_a_volume_is_heard_once_each`
as flaky after it passed alone (the first run said package `zone` was not
running). That observation remains in the gate log; no production correction
or known-failure waiver is claimed for it.
