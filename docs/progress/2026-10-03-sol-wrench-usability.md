# 2026-10-03 Sol Wrench authoring usability

Wrench lane for `codex/v0.2.2-hardening`; root owns integration and release. The creator journeys in `docs/audits/creator-and-npc-hardening-plan.md` guided the work. This entry concerns UI authoring, not new rule evaluation semantics or NPC behavior.

## Delivery

- Legacy qualified inputs are grouped only when their base input also exists. The shared popup helper retains each original name and selected ID; current selections remain visible and word search reaches hidden variants. No automatic TeamN-to-IF conversion. A held-out test searches `activate team6`, chooses the exact original input and sends its unchanged 900 ms delay with no invented guard.
- Search aliases add common gameplay vocabulary (click, touch, ball goal, door, points, checkpoint) while labels, providers and event identities stay unchanged. Outputs use existing provider identity/friendly Add-On names for disclosure; no Slayer-name special case. Unknown Add-Ons retain searchable fallback choices.
- Only rows with IF checks expand. A basic neighboring event keeps its classic input/target/output geometry. New +IF is explicitly unfinished until the creator chooses a check; it cannot silently send the UI placeholder `Self Exists`, `Player Alive`, or an invented progress key.
- Raw unfinished parameter/condition text and pending checks survive rebuild, resize, cancel/return, row copy and cross-brick Copy. Cross-brick Copy clones UI drafts rather than serializing them. Destination opaque host tokens remain destination-owned; source opaque rows never cross bricks. An unavailable copied input/output stays visible and editable with its original label and blocks Send until corrected or explicitly removed; it never becomes a fabricated destination preservation token.
- Incomplete entered rows block Send with row-specific feedback. Untouched trailing blank rows remain harmless. A missing named target or exact Spawner selection is explained without omitting the entered row. Named identities remain visible when absent from refreshed lists.
- Unavailable preserved host rows remain read-only but now have individual Remove row actions. A regression confirms another opaque token and a delayed editable row survive unchanged.
- Exact-ball goal authoring/copy uses independent `MatchBall` and `PracticeBall` spawner names; delayed puzzle authoring retains explicit variable keys/values and delay. Team, Spawner, Type and Points labels replace generic value captions where those meanings apply.

## Native controls and bounded visual evidence

The renderer blits whole `GuiBitmapButtonCtrl` images; it does not nine-slice them. Detection region formerly stretched the native button2 91×38 bitmap into 185×30. An intermediate natural button1 **211×38** correction restored border proportions, but root visual review found it still too long beside the other buttons. Final delivery uses existing button2 at its natural **91×38**, labelled `Region...` / `Hide region`, aligned with the **91×38** footer buttons. The open panel still says Detection region. The inserted row is 42 pixels tall. Existing semantic footer placement remains intact in all three Wrench variants.

Compact +IF, Copy row, Remove row, Explain saved and condition X controls formerly squeezed button2 into 55/75/90/100×26 or 23×24. They now use existing scalable `GuiButtonCtrl`/`GuiButtonSmProfile` controls (native Arial 14 profile), keeping compact geometry without warped bitmap artwork. Detection help stays two short lines.

Ignored native capture generates `artifacts/ui-native-wrench/` with basic, pending IF, mixed basic/guarded team door, exact-ball goal, grouped/search input menus, grouped outputs, normal/sound/vehicle and expanded detection views. It checks missing textures and renders 640×480, 1024×768 and 1920×1080 at supported requested scales 1/2, plus physical 400×300 using the normal automatic minimum-canvas scale. Requested 2× at 1024×768 is clamped by existing UI fit policy; 400×300 scales down and is not claimed pixel-crisp. Separate native bounds regression also covers direct logical 400×300 and both expanded/collapsed states.

Representative reviewed files: `VehicleSpawn.png`, `VehicleSpawn-Detection-1024x768-1x.png`, `VehicleSpawn-Detection-400x300-1x.png`, `Events-Mixed-1024x768-1x.png`, `Events-PendingIF-1920x1080-2x.png`, `Events-Input-Grouped-1024x768-1x.png`, `Events-Input-Search-1024x768-1x.png`, `Events-BallGoal-1024x768-1x.png`.

## Verification and failures

- `cargo test -p bri-ui --lib`: **189 passed, 7 ignored**, `/tmp/bri-v022-sol-wrench-ui-freeze.log`. Includes pending check/copy, raw number recovery, grouped legacy identity, exact-ball/delayed puzzle, cross-brick unavailable-provider and preserved-row removal regressions.
- `cargo test -p bri-ui --test field_flow -- --include-ignored --nocapture`: **8 passed** in synthetic and generated-content modes, `/tmp/bri-v022-sol-wrench-field-flow-freeze.log`. Only Wrench expectations changed: entering a row then resetting Input/Target cannot silently send an incomplete row; explicit missing-output feedback is asserted. Copy-lock and all other field coverage remain active.
- `cargo test -p bri-ui --lib screens::wrench::tests::region_row_preserves_authored_controls -- --ignored --nocapture`: **1 passed**, `/tmp/bri-v022-sol-wrench-native-bounds.log` (same final geometry).
- `cargo test -p bri-ui --lib screens::wrench::tests::authored_wrench_offscreen -- --ignored --nocapture`: **1 passed**, 2.39 s, `/tmp/bri-v022-sol-wrench-native-freeze.log`; final artifacts include the preserved-row Remove action.
- `cargo clippy -p bri-ui --all-targets -- -D warnings`: **passed**, `/tmp/bri-v022-sol-wrench-clippy-freeze.log`. Touched-file `rustfmt --edition 2024` and `git diff --check` passed.
- Earlier work-in-progress library compiles caught a BTreeMap test fixture `.push`, a helper mistakenly inserted in the Wrench rather than WrenchEvents impl, and missing View-event arguments in the new capture fixture. These were corrected before final verification. First field_flow run correctly exposed three prior silent-incomplete-row assumptions; updated expectations now require rejected Send and retained drafts, rather than dropping test coverage. Initial clippy requested `?` in the draft clone filter, corrected without suppressions. Rustfmt was initially invoked with edition 2021 and rejected existing let chains; touched-file format uses repository edition 2024.

Final compact Region adjustment: `cargo test -p bri-ui --lib screens::wrench::tests -- --include-ignored --nocapture` passed **26/26**, including native capture/bounds (`/tmp/bri-v022-sol-wrench-region-final.log`). Final compact normal/vehicle and physical 400×300 expanded captures were inspected. Warnings-denied all-target UI clippy also passed on that final adjustment (`/tmp/bri-v022-sol-wrench-region-clippy.log`).

No original content was changed or committed. No visible game or interactive play was launched. Offscreen art checks and in-memory input tests do not prove creator speed or multiplayer host execution; Maxwell's interactive acceptance and root's combined full gate/platform release remain outstanding.
