# 2026-10-11 v0.2.9 Add-On compatibility: script math, legacy prints, rule pushes, DTS blend, item coverage

Release blocker 5 of v0.2.9 (Add-On compatibility and the Rallypack/P90
import tooling), from Max's v0.2.9 handoff
(`docs/audits/v029-legacy-addon-porting-handoff.md`) and his reports of the
printer refusing an old save and the soccer crash. Branch
`claude/v029-addon-compat-1khpmi`, PR #41. Subagents ran as Explore agents:
GPT-6.1 Sol, which Max asks for, is not available in this environment.

## What changed and why

**Script math as a class, not `ceil` alone.** Rallypack's distant-shot sound
failed on every shot in v0.2.8 with "Function not found: ceil (f64)". Rhai's
float functions take only floats and name rounding `ceiling`, so any whole
number passed to float math, and any C/Rust/JavaScript spelling, failed the
first time the line ran, mid-game. `crates/package-runtime/src/script/math.rs`
gives every float function a whole-number overload (read as a float), makes
rounding a whole number return it unchanged, and adds the usual aliases
(`ceil`, `trunc`, `fract`, `pow`, `log10`, `log2`, `clamp`). Rhai's own names
are unchanged.

**Unknown calls warn when the Add-On loads.** `script/lint.rs` walks each
compiled script and checks every call's name and argument count against the
game's functions and the script's own. One that cannot resolve is a
`script.unknown_function` warning at its line, with a "did you mean" or
"takes N arguments" hint, in `check` and in the client's Add-On health. A
survey of all 28 repository `.rhai` files found no false positives; the only
hits are the placeholders in the Rallypack template, which the recipe fills.

**NOPRINT is no print; the printer opens on any print.** v20 saves write
`NOPRINT` for a brick without a print. The BLS reader kept it as a print
named NOPRINT, and the printer refused to open on any brick whose print it
could not resolve ("Current brick print is not bounded to native content").
`brick_materials::is_no_print` now names the sentinel for the reader and the
renderer, and the printer opens with nothing highlighted when it cannot
resolve the current print. Nothing depended on the refusal: the server's
SetPrint records the old print for undo either way.

**A goal pushes the ball while its builder is dead.** The soccer report's
"Rule owner may not move this object" was a wrong rejection, not the crash:
`setObjectVelocity` and `resetObject` checked the physics-gun rule
(`may_move`, which needs the builder alive and connected) instead of the
documented objects rule (DESIGN.md rule 8: the object came from the builder's
spawn bricks, or from someone the builder lets edit its events). The crash
itself belongs to the "Soccer crash" thread.

**DTS: only translucent materials blend.** The P90's black body was invisible
because its materials are flagged additive (8) without translucent (4), and
the importer drew them additive. Torque blends only translucent materials. A
census of all 154 v20 shapes and a community sample found no stock material
with 8 set without 4; stock additive materials are 0x4f or 0x6f. Such a
material now imports opaque, with a note.

**Every stock texture in base/data/shapes is in the item presentation.** The
stock pack carried 6 of v20's 25 (only those stock models draw). The importer
binds an Add-On material to a stock texture by its key only when the installed
game has it, so black75 resolved and black25, red, yellow and gray50 fell back
to a stand-in. `build_presentation.py` now packs all 25; the next bootstrap
rebuilds the pack because its script changed. When the installed game lacks
one, the import report now says so ("uses v20's ..., which is not among the
installed game's item textures") instead of "has no texture".

**A coverage level per item.** `import-report.json` (schema 3) has `items`:
each datablock with a `uiName`, rated by what it is made of (the datablocks it
names, their files, the Add-Ons they come from, their script callbacks) as
preserved, ported, approximate, unsupported or missing_source, worst first,
with reasons. `IMPORT-REPORT.md` has an Items table. So the report can tell
faithful from different, shape readings that match Torque now go to the
converter's `faithful` list and stay notes; records needing adaptation become
the asset's `gaps`, and stock art the installed game lacks its `missing`.
Documented in `docs/modding/porting.md`.

**The Rallypack/P90 recipes are in Git.** They lived only on Max's PC
(local branch `codex/rallypack-p90-v028`, worktree
`BlocklandReImagined-worktrees/rallypack-p90-v028`). The branch tip's 15
files are committed byte-identical, authored by Max; the branch and worktree
are untouched. The six tests that need the private packs are `[[skip]]`
entries in `tools/gate-known-failures.toml`, and two clippy findings in the
P90 test are fixed.

## Evidence

- `cargo test -p bri-package-runtime` (math and lint unit tests,
  `a_call_to_no_function_is_a_warning_at_its_line`), `-p bri-sim --test
  script_api` (28 passed, including the ceil-on-fire and unknown-call tests),
  `-p bri-sim` lib (369 passed), `-p bri-bls`, `-p bri-content`,
  `-p bri-client --lib -- tool_ui world_scene world_chunks` (34 passed),
  `-p bri-addon-import -p bri-convert` (all passing), `-p bri-net --test
  rallypack --test rallypack_p90` (compile; 6 ignored, as they need private
  packs).
- `cargo clippy -D warnings` on bri-addon-import, bri-convert,
  bri-package-runtime, bri-bls, bri-content, bri-sim, bri-client and bri-net
  tests; `cargo fmt --all -- --check`.
- New regression tests cover each class: math overloads and aliases, the
  unknown-call warning in `check` and in a hosted game, NOPRINT in the reader
  and printer (five save spellings), a goal pushing while its builder is dead
  and for an administrator who may edit its events, blend flags, and items at
  each coverage level (`each_item_gets_the_worst_coverage_of_what_it_is_made_of`,
  plus ported and unported shotgun coverage in `ports.rs`).
- Not run here: the full PC gate, `bri-client --check` against real content,
  `build_presentation.py` against the v20 reference (needs the originals),
  and the Rallypack/P90 recipes and their tests (need Max's ZIPs on E:).

## Verified on Max's PC (headless, 2026-10-11)

Run through Remote Control in a worktree of this branch; no window, main
checkout untouched, no push from the PC.

- `python tools/bootstrap.py --v20 <reference>` exit 0, `bri-client --check`
  passed. It rebuilt item-presentation-pack-010 and also weapons-pack-009,
  vehicles-pack-012 and worlds-pass-006: their steps hash bri-convert's
  sources, which the shape `faithful` split changed. The new presentation has
  93 textures, all 25 `base/data/shapes` keys among them.
- `build_rallypack_p90.py` and `build_rallypack.py` built into fresh folders
  (48 items, 207 images, no unresolved materials); `rallypack` 1/1 and
  `rallypack_p90` 5/5 passed with the private packs (with
  `base-packages.json` copied into the content folder as `packages.json`;
  the tests' headers now say so).
- `bri-import-addon` on the real ZIPs with `--reference` and `--installed`:
  Weapon_Rallypack 6 preserved, 4 unsupported, 45 missing_source (each names
  an icon file absent from the Add-On and the reference); no asset `missing`.
  Weapon_Rallypack_P90: the P90 is unsupported (`onFire`, `onDryFire`,
  `onPreAmmoCheck` need native behaviour: the recipe's host rules, not a
  catalog port, carry them). Seven of `p90.dts`'s eight materials import
  opaque; `red` stays additive (it sets translucent too, so Torque blends it).
- `script_api` 28, sim `rules` 43, `bri-bls` 14, client `tool_ui`/`world_scene`
  27 passed against the real content. The 35 v20 `.bls` saves contain no
  NOPRINT, so the print fix is covered by the unit tests, not those saves.

## Uncertain

- That v20 drew Add-On materials from base/data/shapes is inferred (the
  importer's existing fallback and Max's P90 recipe both assume it), not
  measured in v20. Earlier notes treated the Hookshot's black50 as drawn in
  the colour shift.
- The Butterfly Knife warnings and the bots' `Empty slot` warning were not
  investigated; the latter belongs to the bots thread.
- Dependency closure and a single build/check/package/install command are
  not built yet; the per-item coverage is the first part of that workflow.

## Next

- Max does no playtest before v0.2.9 (2026-10-11); the release gate is the
  integration thread's automated run.
- Retire the P90 recipe's own stock-texture copy once the rebuilt pack is in.
