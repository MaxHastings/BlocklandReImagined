# 2026-10-03 Sol host colorset chooser

Root owns client file discovery, parsing, palette application and release integration. This UI lane follows Maxwell's request for a simple Colorsets button and radio chooser instead of another persistent Start Game dropdown. No original colorSet.txt, downloaded source or generated content is committed by this lane.

## Behavior

- Start Game has a compact **Colorsets...** button cloned from Start Game's existing rounded Add-Ons button profile/bitmap and selected-name summary in the free row above its existing Add-Ons/Advanced Config/Music Files actions. Existing launch, server-name/password and player-count fields retain their identity and behavior.
- A separate native **Colorsets** window offers radio choices from the local host catalog, a bounded selected-palette swatch preview, **Folder...**, **Cancel** and **Use**. The dialog sizes to the catalog and largest preview (190–300 px high), rather than leaving a large empty list below three choices. A 64-swatch preview fits in 194 px with all three natural 91×38 action buttons. A large catalog scrolls within the window. Browsing changes a local draft; only Use writes `$Pref::Server::ColorSet` and queues SaveSettings. Cancel/Escape do not change the saved palette.
- Selection follows the stable catalog ID across refreshed/reordered catalogs, including identical display names. Removed saved/draft choices remain explicitly unavailable, without silently choosing Default. Start Game blocks Launch, including a direct host command, until the saved choice is available; the chooser blocks Use for a missing draft.
- Opening Start Game and the chooser requests a refreshed local catalog. Folder uses the explicit action bridge; root refreshes on folder completion and focus regain. Incoming catalogs preserve the local draft and the Start Game raw server-name/admin-password/player-count fields.
- Preview reads the catalog's color divisions and caps display at 256 swatches. It does not change the authoritative world palette or introduce a new content kind/protocol. Root resolves the saved ID freshly before hosting.

The earlier dropdown implementation was replaced before verification/publication; no dropdown behavior is claimed as delivered. UI fallback catalog contains Default, while root supplies actual Default and installed/custom choices. Root owns the exact Trueno source and packaging evidence.

## Small-window fit

The requested interface scale now also scales down to the established 640×480 logical minimum when the physical window is 400×300. Previously an explicit 100% preference clamped the fit to 1.0 and could clip the authored Start Game canvas, although automatic scaling fit it. This is a narrow fit correction with an exact requested-scale regression; larger requested sizes remain bounded by available logical space.

## Verification

The initial complete chooser implementation passed:

- `cargo test --locked -p bri-ui --lib`: **195 passed, 8 ignored**, `/tmp/bri-v022-sol-colorset-ui-lib.log`; includes chooser draft/Cancel/Use, duplicate-label reorder/removal, folder/scroll, Start Game raw-field refresh/host payload/missing-choice and requested small-window scale regressions.
- `cargo test --locked -p bri-ui --test field_flow --test runtime_input --test minigame_screens`: runtime input **34 passed** (including explicit refresh action on Start Game entry), `/tmp/bri-v022-sol-colorset-ui-integration.log`.
- `cargo test --locked -p bri-ui --test field_flow --test minigame_screens -- --include-ignored`: **8 field-flow and 24 MiniGame passed**, `/tmp/bri-v022-sol-colorset-content-integration.log`.
- `cargo test --locked -p bri-ui --lib authored_start_game_colorsets_fit_and_render -- --include-ignored`: **1 passed, 1.16 s**, `/tmp/bri-v022-sol-colorset-native-capture.log`. Produced 16 native screenshots at 400×300, 1024×768 and 1920×1080 requested scales 1/2, including stock preview, example custom preview and missing selection. The fixture uses the actual imported UI/stock palette and explicitly named example custom palettes; it is not evidence of exact Trueno discovery or palette application.
- `cargo clippy --locked -p bri-ui --all-targets -- -D warnings`: **passed, 8.91 s**, `/tmp/bri-v022-sol-colorset-ui-clippy.log`.

Offscreen inspection exposed the unnecessary empty area and rectangular Start Game button. Root approved the compact height calculation and cloning the existing rounded button, implemented after this first batch. Root also approved removing the second exact duplicate guarded `js_sortlist`/`js_sortnumlist` match arm in menus.rs; the first identical arm retains behavior. Source review found that popping a chooser does not refresh the underlying screen: the saved preference changed immediately, but the summary/Launch state waited for a later save acknowledgment. `NativeScreen` now caches the shown colorset ID and refreshes on the next UI update, like its existing game-mode refresh. The actual in-memory UI regression opens Colorsets, selects a radio choice, uses it and launches before any save/catalog acknowledgment arrives, preserving the entered raw server name.

Final source verification:

- `cargo test --locked -p bri-ui --lib screens::menus::tests -- --include-ignored`: **6 passed, 1.96 s**, `/tmp/bri-v022-sol-colorset-final-menus.log`. Includes duplicate Join Server sort behavior, missing-choice and raw-draft refresh checks, actual no-ack chooser/Use/Launch workflow, and **20 fresh authored native captures** at 400×300, 1024×768 and 1920×1080 requested scales 1/2.
- `cargo test --locked -p bri-ui --lib screens::host_colorsets::tests`: **4 passed**, `/tmp/bri-v022-sol-colorset-final-chooser.log`. Includes the exact **64 swatches / 194 px** dialog and natural 91×38 action-button bounds, large catalog height cap, draft/Cancel/Use/reorder/missing behavior and folder/scroll continuity.
- `cargo clippy --locked -p bri-ui --all-targets -- -D warnings`: **passed, 5.58 s**, `/tmp/bri-v022-sol-colorset-final-clippy.log`.
- `git diff --check`: passed. Changed source was formatted with edition 2024; no workspace formatting or dependency changes.

Visually inspected final Start Game/chooser captures at 1024×768 and 400×300. The stock short-list chooser is **380×190**, the large unused lower area is gone, and the Start Game button uses the same rounded bitmap/profile as its adjacent original controls. Capture paths: `artifacts/ui-native-host-colorsets/StartGame-choices-1024x768-1x.png`, `StartGame-selected-1024x768-1x.png` and `StartGame-choices-400x300-1x.png`. An intermediate half-edited dropdown build had a missing local `named` helper; the finished button/dialog implementation compiled cleanly in the above batch. No interactive or visible game was launched.


## Handoff limits

UI source is frozen and the Cargo slot released to root after the final checks. Root's client lane owns local file discovery/default/Trueno identity, parser validation, fresh host-ID resolution, palette application and saved-world rejection. This UI fixture's example palettes do not establish exact Trueno content fidelity, human usability acceptance or published cross-platform behavior; root's combined gate, Windows checks, platform packaging and Maxwell's interactive test remain separate. No commit, push, branch deletion, manifest/lockfile edit or visible game occurred in this lane.
