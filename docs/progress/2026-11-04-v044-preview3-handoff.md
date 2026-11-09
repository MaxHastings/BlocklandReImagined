# v0.4.4-preview.3 AFK handoff

Playable folder: `E:\Downloads\BlocklandReImagined-v0.4.4-preview.3-windows`.
Launch `Launch.cmd`; the binary reports `v0.4.4-preview.3 (c76b31a18)`.
Earlier previews and user saves remain available. No interactive playtest was
automated. The destruction PR #437 remains held for Maxwell's motion judgment.

QA PR #440 merged as `21f74083629eccca8d5d51a34319586b432101ba` after all
seven executed platform checks passed on exact head
`c2e2fabb0e47e79438262eb2fe129bf5b411a361`; the conditional Windows test was
skipped. The fresh Actions check-run receipt is `qa-final-ci.json` in the AFK
evidence directory. Main is integrated into this candidate without replacing
recent work. The automatic merge duplicated four identical manual gate
entries; integration removes only those duplicates, retaining each probe and
the destruction repro once. History/protocol sanity and gate-tool tests pass.
No normal-game behavior changed after the preview.3 source build; later
changes are offscreen tooling, integration and documentation. #437 still needs
its final-head CI and Maxwell's motion judgment before any merge.

Compared with preview.2, this includes the bounded collision-work scheduling
correction from the [AFK destruction receipt](2026-11-04-afk-destruction-scheduling.md).
Preparation and motion no longer repeatedly favor the same prefix of a large
section. Collision service accounts for fixed physics steps rather than only
display frames, preserving the per-simulation-second budget. This is a
fairness correction, with no increased body cap or arbitrary collapse spin.

All 34 focused section tests and both private two-stock-rocket saves across
six cadence/body-budget configurations pass. Fourteen default-budget real
geometry captures pass, with representative first-contact frames inspected.
The highest measured fallback debt is 66.667ms, below the retained 100ms cap;
every replay fires two projectiles and drops zero authority cues. These
technical checks do not establish that the resulting motion feels natural.

The three zombie saved pursuit checks and 22 ordinary bot-brain integrations
pass. No additional bot production change was made during the AFK pass: a
tie-order experiment failed the tower assertion and was reverted. Expensive
balcony detour searches can still pause for several simulated seconds. This
remains a known planning limitation, not completed instant pursuit.

Two additional 60-second diagnostic traces preserve ordinary controls and
vulnerability. In the house-exit save the single zombie moves after eight
ticks, reaches and kills the host, with no uncredited death. In the cube save
all three begin moving after roughly six seconds, none reaches the host's
floor and no kill occurs; repeated route attempts remain. Reachability of the
saved host position by ordinary controls has not been established, so the
cube result is unresolved, not a passing pursuit assertion. Trace-only test
completion does not imply success. Evidence folders are `zombie-house-exit`
and `zombie-cube` under the AFK evidence root.
Read-only decoding confirms the cube save has five authored bricks: a 16x
cube, player spawn and three zombie holes, without an authored staircase.
Surrounding map reachability remains unproven; this does not justify a
content-name workaround or a fabricated jump capability.

The [vehicle/loading receipt](2026-11-04-afk-vehicle-and-load-probes.md) records
20 Speedkart transition captures, seat integration checks, two cone GPU tests
and first/repeated load timings. First loads miss the five-second target;
all measured repeated loads meet three seconds. Full-workspace, exhaustive
map matrices, soaks and native interactive fidelity acceptance are not claimed.

Build command: `BRI_VERSION=v0.4.4-preview.3`, then
`cargo build --locked --profile playtest -p bri-client --bin bri-client
-p bri-addon-import --bin bri-import-addon -p bri-net --bin bri-server
-p bri-package-runtime --bin bri-addon-check -p bri-sim --bin bri-addon-run`.
The explicitly invoked map-load helper was also built separately.

Packaging used `tools/package_playtest.ps1` with the exact executable SHA,
the previous private-preview import bundle, the pinned shader compiler archive,
and `-SkipArchive -PreviousImportsForPrivatePreview`. Verification with the
same private-preview flags passes all 13,458 files and ten reference menu
images. Hidden, awaited packaged `--version` and `--check content` both exit0.
The latter validates 21 maps, 1,372 brick definitions, 35 save pictures and
14 modern-light maps. It still reports 19 known Add-On health problems and
226 fallbacks; this is a private preview, not a fidelity-clean release.
Executable hash and startup output are in the private evidence directory
`E:\Downloads\BlocklandReImagined-Release-Evidence\v044-preview3-2026-11-04`.

Maxwell's next acceptance: replay both saved two-rocket sequences and judge
the fall, contact and breakup in ordinary gameplay. Recheck the Bedroom
staircase, house balcony and Beta16 tower from their saved player spawns.
The updated author give-back remains
`E:\Downloads\Speedkart-addon-v0.4.4-preview.1.zip`; its API and contents
remain compatible and unchanged. Interactive driving acceptance is still his.
All three native packages also pass preview.3's packaged Add-On checker;
individual checker output is beside the preview startup evidence.
Keep the warm worktree while #437 and follow-up validation remain active.
