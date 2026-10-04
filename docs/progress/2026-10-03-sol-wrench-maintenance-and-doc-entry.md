# 2026-10-03 Sol Wrench maintenance and newcomer entry

Bounded follow-up on `codex/v0.2.2-hardening`; root owns integration and publication. The separate branch-retirement entry records exact historical refs and artifact evidence. No branches or worktrees were removed.

## Behavior-preserving Wrench cleanup

Extracted one private `complete_row_supported` predicate for input support, target class, output support, parameter arity and parameter types. Both `preserve_unsupported` and `copied_unavailable` use it. Their dispositions remain distinct: unsupported imported complete rows retain read-only host-preservation behavior; copied drafts remain editable and block Send when unavailable. Copied partial-row checks and destination named-target permission checks remain unchanged. No catalog, runtime or shared model semantics changed.

Existing regressions cover wrong target classes, unknown parameter specifications, copied unavailable providers, unfinished checks/raw drafts, and individual preserved-row removal. Verification after root's coordinated Cargo slot is recorded below; previous lane passes are not substituted for this extraction.

## Property-window composition follow-up

Maxwell's screenshot requested Region and Respawn on the same row, exposing a composition gap in the earlier proportional-art/bounds review. `region_view` now shares an existing Respawn action row when neighboring space is free; it checks semantic command and actual control rectangles instead of assuming a particular imported/fallback layout. Crowded layouts keep their separate insertion row. The native VehicleSpawn geometry returns from 323×337 to **323×295**: Respawn at (16,200), Region at (115,200), both **91×38**, with an **8 px** gap. Send/Cancel/Events commands and property rows remain unchanged.

The expanded panel retained a 274 px height/minimum 326 px window from older seven-line help. Its actual two-line content ends at 180 px. The panel now fits that content: native Sound stays **221 px** high when expanded (formerly 326), and VehicleSpawn stays **295 px**; Normal stays **415 px** for its original fields. Authored-content regression explicitly checks same-row native dimensions and unchanged compact heights at logical 400×300 and 1024×768, alongside existing all-variant overlap checks. Fresh offscreen rendering and authored bounds checks passed in the coordinated slot below. Visually inspected VehicleSpawn at 1024×768 and 400×300: the action row is together and the old empty row is gone. Inspected expanded Sound at 1024×768: its two-line hint and size controls fit within 221 px. These are rendered layout checks, not an interactive gameplay acceptance claim.

## Newcomer documentation

Updated `README.md`, `docs/README.md`, `docs/TESTER-GUIDE.md` and `docs/rule-workshop/PLAYTEST.md`:

- Replaced obsolete Windows-only install claims with the published Windows x86-64, Apple Silicon Mac and Linux x86-64 platforms. Linux's glibc 2.35 requirement comes from the release workflow and current known issues.
- Distinguished Python 3.11+ for the full development/gate workflow (`tools/gate.py` imports `tomllib`) from bootstrap's own Python 3.9+ minimum. Bootstrap handles Windows/macOS/Linux prerequisite checks; no installer/runtime behavior changed.
- Added a short no-code Wrench/MiniGame → editable Workshop examples → creator test card path before code-based Add-On development. Linked existing guides rather than creating another manual.
- Made platform launcher, state and log locations explicit, verified against `crates/client/src/main.rs`, `crates/crash/src/lib.rs`, the Linux launcher and platform packagers. Mac content copies remain per-build and the Mac guide owns their details.
- Explained that a new +IF needs a chosen check before Send, and that the compact Region... button opens Detection region. Removed README's duplicated list of future seams in favor of its canonical audit link.

STATUS, KNOWN-ISSUES, FEATURES and CHANGELOG remain root-owned. Root was notified of STATUS's older v0.2.0 decision wording, Shark limitations requiring current evidence, and NPC claims requiring final completion/rest tests. Historical progress and acceptance evidence were not rewritten.

## Read-only maintenance recommendations

1. Wrench duplicate complete-row checks: implemented above, with existing behavioral regressions retained.
2. NPC approach-clock duplication: `objectives::State::suspend` and `bot_objective` share elapsed/last-tick/deadline math. A future private helper can centralize it while keeping approach suspension separate from absolute delayed-event waiting. Verify actual rest/resume, combat interruption, ready-bot fairness and unchanged due-time waits; do not fold this into the new behavior fix before it passes.
3. Optional pure relocation of `package-runtime/src/script.rs::register_physics` into a private child module. Preserve registered names, overloads/order, invocation state and capability checks; exercise package-runtime sandbox/sample tests and sim script/tether tests. No new API, framework or dependency is needed. Large-file line counts alone do not justify wider splits.

No confirmed dead production code was identified in these reviewed paths. The unused local client manifest `personal` field is still part of accepted serialized schema, and pure-tested tactics variants are intentional provider seams; deleting either as apparently unused would change behavior. This is a scoped audit, not a claim that the whole workspace has no dead code.

## Independent integration review

Read current importer body-box/mount extraction, avatar swim/accessory selection, script PlayerView fields, bot rest ownership, MiniGame Item/PlayerType bridge and shared grouped/search popup implementation. Body meshes select their own rig before choosing swim; death/sitting override it. Semantic MiniGame types retain the same selected values and validation. Group/search disclosure keeps authored IDs and does not emit value changes for Back/group/cancel. No concrete regression was identified in those paths during source review; this does not replace current actual-content tests.

- Reported importer square-but-different stance widths: stand `1 1 2`, crouch `2 2 1` previously appeared fully converted even though the motor has one width. Root reports a conservative correction with an exact mismatched-width and unknown inherited-width regression; verification belongs to root's importer run.
- Reported Shark policy/permission mismatch: policy recognized every bot using the Shark model, while RestBot ownership uses its immutable bot-kind provider. A foreign kind legally assigned/reusing that model could queue MountObject, then have RestBot refused. `packages.rs` applies operations separately and continues after warnings, so this was a partial-policy path rather than an atomic rejection. Root exposed immutable `PlayerView.bot_kind`; the Shark owner now requires its own kind namespace as well as the model. Source review confirms that admission guard and a borrowed-body negative fixture; current verification belongs to the owner's coordinated run. Deletion authority remains restricted. No physical reproduction success is claimed here.
- At review time, local `content/addons/bot_shark` was old generated input: archetypes lack current authored dimensions, buoyancy and mount points, and helper speeds remain zero. Its real rig does contain consecutive mount0–3. Root was notified to regenerate current importer output before interpreting final package evidence and subsequently staged a fresh corrected import for the owning lane; that lane records current physical evidence. No original/generated assets were edited by this review.

## Verification

- `rustfmt --edition 2024 --check crates/ui/src/screens/wrench.rs`: passed after predicate extraction.
- `git diff --check`: passed for current edits.
- `/opt/homebrew/bin/python3` local-link/package-guide smoke: **75 repository links passed** across the four changed entry guides; generated all four creator guides in a temporary folder and resolved every local packaged link. Temporary output was automatically removed.
- `cargo test --locked -p bri-ui --lib`: **195 passed, 8 ignored**, `/tmp/bri-v022-sol-colorset-ui-lib.log`. This covers the extracted vocabulary predicate through existing imported/copied/partial-row behavior regressions.
- `cargo test --locked -p bri-ui --test field_flow --test runtime_input --test minigame_screens`: synthetic field-flow **4 passed/4 ignored**, MiniGame **23 passed/1 ignored**, runtime input **34 passed**, `/tmp/bri-v022-sol-colorset-ui-integration.log`.
- `cargo test --locked -p bri-ui --test field_flow --test minigame_screens -- --include-ignored`: **8 field-flow and 24 MiniGame passed**, including actual-content paths and native MiniGame captures, `/tmp/bri-v022-sol-colorset-content-integration.log`.
- `cargo test --locked -p bri-ui --lib region_row_preserves_authored_controls -- --include-ignored`: **1 passed**, `/tmp/bri-v022-sol-wrench-same-row-bounds.log`; same-row dimensions/heights and all supported variants remain covered.
- `cargo test --locked -p bri-ui --lib authored_wrench_offscreen -- --include-ignored`: **1 passed, 2.49 s**, `/tmp/bri-v022-sol-wrench-same-row-capture.log`; current native screenshots in `artifacts/ui-native-wrench`.
- `cargo clippy --locked -p bri-ui --all-targets -- -D warnings`: **passed, 8.91 s**, `/tmp/bri-v022-sol-colorset-ui-clippy.log`.
- The subsequently approved Colorsets composition adjustment and duplicate Join Server sort-arm removal are recorded and checked separately in `2026-10-03-sol-host-colorset-chooser.md`; the above passes precede those small edits. That entry records final 6 menu/4 chooser tests, 20 fresh captures and scoped clippy after all UI source changes.
- No visible or interactive game was launched.
