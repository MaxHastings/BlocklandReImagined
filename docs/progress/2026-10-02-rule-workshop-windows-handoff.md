# 2026-10-02 Rule Workshop Windows playtest handoff

Continues the existing `rewrite/rule-workshop` experiment. The delivered source
revision is `8159efcd404bb06d8785081b6b084df70a610888`; this entry records the
handoff after that build. Main was not merged or changed by this work. No
permanent RuleProgram/beta compatibility contract is claimed.

## Delivered creator scope

Eight `/rulelab` recipes plant ordinary editable event bricks: switch, puzzle,
race, hill, slayer, soccer, sandbox and addon. Optional AND guards, bounded
integer state, region/object observations, native scoring/round operations and
an existing-package vocabulary extension share the current event scheduler.
Steel Ball, Gravity Gun and Portal Bricks ship enabled by default. The recipes
are starting points for mutation and combinations, not polished game modes.

`docs/rule-workshop/PLAYTEST.md` gives setup, mutation ideas and Explain usage.
`docs/rule-workshop/DESIGN.md` records twelve material semantic choices, awkward
seams/limits and reusable versus experimental assessment. Both ship beside the
client; the package rewrites the guide's design link to its shipped filename.
`Launch.cmd` selects a separate RuleWorkshop settings/save profile. Source
installations and the normal profile remain unchanged.

## Verification and package evidence

Local final source:

- `cargo test -p bri-events -p bri-ui -p bri-sim -p bri-world --lib --tests`:
  860 passed, zero failed, 136 ignored. Sixteen focused spike tests passed.
- Warnings-denied clippy on affected sim/client/net paths passed; earlier full
  core/all-targets clippy passed too.
- Net library/loopback/state-limits/event-storm sweep: 93 passed, zero failed,
  seven ignored. Native v20 event tests (two), native editor field-flow tests
  (three), and bounded actual-pack offscreen Wrench rendering passed.
- Final release client/importer/server build passed. `bri-client --check content
  artifacts/rule-workshop-check-state` passed: 14 maps, 966 brick definitions,
  all 35 save pictures and no Add-On health issues; no window/audio opened.

The cold initial Windows run 37048646186 passed release build, tests, clippy,
content startup check, packaging and extracted-ZIP checks. It is superseded by
the final run below because it predates the context fixes and isolated launcher.

Final Windows run [37054054989](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/37054054989)
completed successfully at source 8159efcd. Version
`2026-10-02-rule-workshop-02`: release client/importer/server build passed; the full
events/UI/sim/world suite passed **860 tests, zero failures, 136 ignored**;
clippy/all-targets with warnings denied passed; content startup and extracted-ZIP
startup checks passed. The extracted package manifest verified all 9,371 files.
The extracted executable's check again reported 14 maps, 966 brick definitions,
35/35 save pictures and no Add-On health issues, without a visible game window.

Download: [Windows artifact](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/37054054989/artifacts/11248936440).
The Actions download wraps `BlocklandReImagined-windows.zip`; extract that inner
ZIP and run `Launch.cmd`. This is an artifact-only branch build, not a public
release. The normal Actions artifact expires after its 30-day retention; the
local copy remains available.

Local ZIP: `/Users/maxhastings/Documents/BlockReImagined/dist/rule-workshop-windows-playtest/BlocklandReImagined-windows.zip` (135,151,546 bytes).
SHA-256: `1ee673a58c5b1adcb0befe63d841ac2aa638a8bca24515de11f9b00759f67121`.
After download, an independent Python ZIP/manifest check verified CRCs, safe
unique paths, all listed sizes/SHA-256 values, no unlisted files, real Windows
executables, Toys package presence, launcher profile and bundled guides. The
packaged design link and enabled portal/gravity-gun/steel-ball vocabulary were
also inspected. The latest three guides, including the new creator test card,
are copied separately beside the ZIP under `dist/rule-workshop-windows-playtest/docs/`.
The bundled guides predate that documentation-only creator-card follow-up; the
gameplay build is unchanged. No validation failures remain in this pipeline.

No interactive gameplay was automated. Maxwell's creator playtests remain the
next evidence required: ordinary door authoring, mutations, unexpected combined
modes, multiplayer context/team changes, delay/Reset behavior, and region/portal
intuition. Strong reuse candidates are the existing scheduler, canonical game
operations, optional due-time guards and shared core/package vocabulary.
State lifetime, team-member totals, region sweeps, attribution and authoring/
Explain presentation remain experiments. Do not infer a full alpha signoff or
permanent architecture acceptance from this handoff.
