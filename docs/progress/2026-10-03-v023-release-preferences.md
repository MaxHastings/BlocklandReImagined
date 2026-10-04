# 2026-10-03 v0.2.3 preferences across fresh release folders

Maxwell confirmed extracting every downloaded update into a fresh folder. Root
assigned this lane the preference audit and a patch-only handoff. Platform
principles and the existing seams were reviewed: stable semantic IDs and the
canonical package planner remain the mechanism. No world/save migration,
manifest/lockfile change, old-release scanning, original-install writes or
interactive testing is added.

## Actual paths and source distinction

The client already chooses a version-independent default state directory:

- Windows: `%LOCALAPPDATA%\BlocklandReImagined`.
- macOS: `$HOME/Library/Application Support/BlocklandReImagined`.
- Linux: `$XDG_DATA_HOME/blockland-reimagined`, otherwise
  `$HOME/.local/share/blockland-reimagined`.

`main.rs` validates the default as absolute. An explicit second positional
client-state directory still opts into isolated/portable state. `App::load`
resolves it once, so working-directory changes cannot switch state underneath
identity, settings or administration. `settings.json` already stores normal
preferences and `$Pref::Server::ColorSet` as an ID. UI Use in the colorset
chooser requests `SaveSettings`, which atomically replaces that per-user file.
There is no demonstrated current-source reset of video/controls/settings on
fresh extraction; Maxwell did not report a specific such reset.

The actual Windows zip includes `Launch.cmd` -> `Launch-Playtest.ps1`, which
launches `bri-client.exe --run .\content` with **no explicit state argument**.
Its package-relative stdout/stderr logs are independent of user settings. The
Linux `launch.sh` likewise passes only `--run ./content`. The macOS `.app`
executes the same client, which resolves content from bundle Resources and user
state from Application Support. Headless probes/tests may deliberately supply
temporary state directories; that does not describe these packaged launches.
`--check` selects the same default state unless overridden, but must not replay
or write the new Add-On preference overlay. Its already documented optional
checkout-default installation remains unchanged.

The concrete reset is Add-On enablement: the ordinary Add-Ons commands rewrite
`content/packages.json` and `content/packages-disabled.json` via
`Library::plan/apply`. Each freshly extracted release starts with its shipped
lists; no per-user choice exists. Package content remains specific to that
release, so copying old PackageEntry objects/directories would be incorrect.

Colorset availability is separate. `user:<filename>` resolves under the stable
state `colorsets` folder. `addon:<folder>` resolves under that installation's
`content/addons/<folder>/colorSet.txt`. A custom palette copied into only an old
release can disappear in the new release even though its selection ID survives.
Existing UI keeps the missing ID visible and refuses direct launch until another
valid choice is selected; the host rereads the current catalog file before
creating the world. The patch does not invent a stale saved palette, silently
fall back, copy absent Add-On assets or change colorset identity. The Colorsets
Folder button already opens the stable per-user location.

## Proposed correction

`/tmp/bri-v023-preferences.patch` adds a bounded, versioned
`state/add-on-choices.json` containing only explicit package-ID -> enabled intent
and whether Default was explicitly requested. Existing UI SetAddOnEnabled and
DefaultAddOns actions go through small wrappers that retain their ordinary
canonical command result and save its actual outcome. Implied dependency changes
are kept by ID; companions continue to follow their owner. Unknown/unavailable
preferences survive unrelated toggles. An explicit Default clears old overrides
and applies the current default policy, so future release defaults are handled
without freezing a previous catalog.

Actual `--run` installs checkout defaults as before, then replays saved intent
through current `Library::plan/apply` before `App::load`. Unspecified fresh
shipped Add-Ons keep their shipped state. Disable plans run before enable plans;
current dependencies and load order are resolved canonically. Missing IDs and
rejected/changed dependency outcomes produce existing-console warnings and keep
the saved intent. Read/parse/schema/size errors preserve the file and surface a
real error; filesystem write failures remain errors. Both preferences and package
lists use their existing atomic write paths. A preference write failure after a
successful local package edit is reported; that edit is not claimed durable.

This persists choices made with the corrected version. It cannot retrospectively
recover toggles from a discarded prior release folder: no record exists outside
that folder. Root explicitly excluded old-release scanning/migration machinery.
World/save schemas and content pack compatibility are unaffected.

## Verification and remaining work

Four proposed CPU filesystem tests use real package lists/manifests, ordinary
Add-On commands and the current Library planner. They cover fresh releases with
changed package paths, dependency load order, explicit off and unspecified newly
shipped on choices; missing/rejected IDs retained through another toggle and
restored when available; absent preference/default-reset/portable-state isolation;
corrupt data preserved before mutation; and settings/selected colorset IDs retained
with a precise distinction between unavailable release-local palette data and
stable user colorset files. The fresh-release test explicitly asserts the current
pre-overlay reset before verifying corrected replay.

`rustfmt --edition 2024 --config skip_children=true --check` on proposed files and
`git apply --check /tmp/bri-v023-preferences.patch` passed. No Cargo run was made
without root's compute lease. Root should run
`cargo test --locked -p bri-client add_on_choices::tests -- --nocapture`, the
relevant existing Add-On/settings/colorset tests and scoped strict Clippy. A full
current-release before/after negative control for the new replay seam can disable
only that call in the fresh-release test; the baseline reset is already asserted
through actual Library reads, not mocked entry insertion. Launch/package/release
verification and Maxwell's two-fresh-folder playtest remain open.

## Root executed evidence

Root applied the patch and ran the focused library target with `--lib`:
`/tmp/bri-v023-preferences-tests.log` reports **4 passed, 0 failed**, 0.14 s.
All four cross-release, retention/default/isolation, corruption and colorset
source-distinction regressions passed. This lane read the result; root owns
execution and remaining Clippy/package/human acceptance.

Root final preference verification passed 5/5 (`/tmp/bri-v023-preferences-final.log`), including protected-role warning coverage. The earlier four focused tests also passed (`/tmp/bri-v023-preferences-tests.log`).
