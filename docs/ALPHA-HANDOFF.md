# Blockland ReImagined — full continuation handoff

Prepared 2026-09-27 at Maxwell's request. Read this before changing code. This is
a handoff of the original complete alpha objective, the earlier building
playtest, and the live follow-up work. It is not a completion claim.

## 1. Product intent and scope history

Maxwell wants what Blockland v20 should have become: the same soul, recognizable
assets, sounds, maps, character, controls, building interactions and fun, with a
modern implementation underneath. Improvements should feel natural. Do not
replace the original art direction or turn toy physics into realism experiments.

The project is a rewrite in Rust/wgpu, currently using Rapier plus a custom
gameplay motor. Jolt was an initial suggestion, not a requirement. Physics must
serve Blockland movement, jets, skiing and vehicle feel. Maxwell explicitly
authorizes sensible technical pivots without asking again about old preferences.

Assets are converted offline into native versioned packs. The runtime must not
depend on Torque, compiled legacy formats, or the original install. Keep the
converters repeatable: "one time migration" is not an excuse to lose the ability
to fix and regenerate conversion mistakes.

There are THREE distinct scopes; do not conflate them:

1. The earliest persistent goal described a minimum alpha with Bedroom/Kitchen/
   Slopes, one weapon, Jeep, core building and an event subset.
2. Maxwell expanded the actual full alpha to **all vanilla core content and
   behavior**. `alpha-contract.md` and `vanilla-coverage.md` record this. The old
   persistent goal text is not authority to shrink that expansion.
3. Maxwell then authorized an earlier **core building playtest first**, with
   unfinished combat/vehicles/minigames/etc. clearly disclosed. This was shipped.
   That temporary handoff did not cancel the complete vanilla objective.

Immediate priority remains fixing playtest blockers and movement feel, not
uncontrolled expansion. Complete the full contract subsequently. Keep the full
goal incomplete until its actual requirements are met. The successful initial
package/push is a milestone, not evidence that the full game is finished.

## 2. Non-negotiable working rules

- Read `AGENTS.md`, `alpha-contract.md`, `playtest-contract.md`, and the latest
  entries in `progress.md`. Earlier entries are historical, not current status.
- **Maxwell does all interactive testing.** Never drive his mouse, gameplay keys,
  menus or desktop, launch a visible game to test, or play audio. Use headless
  in-process input, silent audio, automated tests and bounded offscreen renders.
- Primary original install, strictly read-only:
  `E:\Downloads\B4v21Launcher\versions\Blockland v20`.
  Earlier `C:\Users\Maxwell\Desktop\Games\B4v21-Launcher-Release\versions\Blockland v20`
  is secondary evidence/stress-test input, not the vanilla inclusion rule.
- Never commit original/generated content, recovered/decompiled scripts,
  research clones, keys, identities, certificates, or build artifacts. Keep
  `.research/`, `content/`, `artifacts/`, `dist/`, state and nested targets ignored.
- Only **GPT-6 Luna** subagents are permitted. Parent integrates and reviews.
  Do not resume old Astra subagents or send Opus additional work by assumption.
- **Another agent is actively changing movement/prediction in this same folder.**
  Maxwell confirmed this during handoff. Preserve its changes and coordinate
  ownership before editing sim/client/network integration. Do not reset/stash
  the whole tree or commit another agent's unfinished work blindly.
- No structural fracture/collapse, generic TorqueScript VM, arbitrary community
  add-on compatibility, public matchmaking/account backend, mod SDK/language
  decision, or registry. Modding is deferred until vanilla is satisfactory.
- Native events and familiar vanilla damage/fake-kill/respawn remain gameplay
  scope; do not confuse those with excluded structural destruction experiments.

## 3. Workspace, Git and delivered package

Workspace: `C:\Users\Maxwell\Desktop\Games\BlocklandReImagined` (PowerShell).

Private repository: https://github.com/MaxHastings/BlocklandReImagined

- `origin`: `git@github.com:MaxHastings/BlocklandReImagined.git`
- Published branch: `main`.
- Published initial commit: `3c90a6e79d3b758a1af6e7b0c51a640279907395`.
- Release source tag: `playtest-2026-09-27-01`.
- GitHub visibility was verified PRIVATE and remote main matched local HEAD.
- Source was explicitly authorized to be pushed for the earlier building
  playtest, superseding the original full-alpha-only publication condition.

Delivered local folder:
`dist/BlocklandReImagined-building-playtest-2026-09-27-01/`

Delivered local ZIP:
`dist/BlocklandReImagined-building-playtest-2026-09-27-01.zip`

Run `Launch.cmd` from the intact folder. No Rust/compiler/original install is
required. Package-local state is `user-state/`; logs are `logs/`. Preserve user
saves, preferences and identity across subsequent builds; do not replace them
with test state. Private `client.identity` must not be shared publicly.

The ZIP is 74,283,235 bytes. Its SHA-256 is
`9640b6a5e1cf38d50ccaa0329dbd7b28072e0b77783087b4b82201f549dc486e`.
The executable SHA-256 is
`a23f67289471afab300c163f3a82ca8685aecfc4fde2522bd098be4a226e855f`.

Package census: 14 selected native packs, 3,147 content files; manifest verifies
3,155 immutable payload files. Packaged `--check` loaded 14 maps and 170 brick
definitions from outside the repository root with fresh isolated state. All ZIP
payloads were independently checked against the manifest. Static CRT imports
were inspected: no VCRUNTIME/MSVCP DLL requirement.

**The shipped folder/ZIP/tag have not been replaced by the local follow-up fixes.**
Do not tell Maxwell the reported crash is fixed in his existing executable.

## 4. Latest user feedback and urgent crash investigation

Maxwell tested the package and reported a crash while holding Shift/crouch or
Space/jump, then clarified it may actually be jetting. He also said character
movement and jetting feel funny. The precise input combination/map has not been
confirmed. Treat a crash fix and subjective movement fidelity as separate work.

### Confirmed launcher defect

All inspected game stderr logs were empty. Root reproduced a separate launcher
bug with a tiny Rust console fixture, never running the game: Windows PowerShell
5.1 converts native stderr redirected with `2>` into ErrorRecords. Combined with
`$ErrorActionPreference = 'Stop'`, the old launcher reported "Could not launch"
and lost the actual error, returning 1 instead of the real exit code.

Local `tools/Launch-Playtest.ps1` now uses `Start-Process -NoNewWindow -Wait
-PassThru` with direct stdout/stderr file redirection. A durable test is in
`tools/tests/Test-PlaytestLauncher.ps1`. It passed both native exit 0 with stderr
and native exit 7, preserving output and exit status, including a path with spaces.
The old package still has the defective launcher. Empty old logs do not prove
there was no Rust error, and cannot reconstruct the lost error text.

### Concrete animation failure candidate and local fix

The native Blockhead `jump` clip is **additive**, priority 8. `armreadyright` is
absolute priority 14; `crouch` is absolute priority 20. The production avatar
assembler sorted by priority alone. `sample_layers` rejects any absolute layer
after an additive layer with `Absolute animation must precede additive layers`.
Therefore jumping with a held-arm overlay or crouch can fail the normal avatar
pose path. This runs even for the hidden first-person body.

Root changed `crates/client/src/avatar.rs` to sort these base layers by
`(animation.additive, animation.priority)` before the existing additive head/look/
action overlays. This is local, uncommitted and not packaged. The original
CPU-only movement regression passed after this change. Expanded native GPU/jet/
held-tool regression work is in `crates/client/tests/movement_crash.rs`, owned
by Luna `/root/luna_crouch_crash`; consult the checkpoint below before rerunning.
The exact reported user crash is not yet proven resolved by this candidate.

Jetting selects the absolute `fall` clip, but transitions from jet release into
upward non-jet motion select additive `jump`; test those transitions, not only
steady jetting. Also test held tool/no tool, crouch combinations, both views and
repeated GPU uploads. Do not just suppress animation errors to stop exits.

### Other local source changes not in the release

- `main.rs`, `settings.rs`, `platform.rs`: restore native saved display size,
  fullscreen and VSync on startup; validate malformed/oversized dimensions;
  fall back to windowed when a saved monitor mode disappears and to FIFO when
  saved no-VSync is unsupported. Audio-device fallback already existed; its
  warning now reaches stderr. Settings tests2 and client all-target strict
  Clippy passed before later concurrent movement edits. Actual monitor switching
  was not exercised. Initial package still always starts 1280x720/windowed/VSync.
- `tools/Launch-Playtest.ps1` and new launcher regression described above.
- `docs/progress.md` includes local follow-up evidence.
- Other agent's currently changing files include `crates/sim/src/prediction.rs`,
  `player.rs`, `simulation.rs`, and `session.rs`. The list can grow. The rewrite
  introduces a collision mirror and a changed Predictor API. It is NOT root's
  verified work and must not be treated as already integrated or passing.

Inspect `git status`, diffs and any running processes first. Do not terminate
Maxwell's user-launched `bri-client.exe` or assume an old PID remains current.

## 5. What exists, and what "exists" does not prove

| Area | Current foundation/evidence | Remaining acceptance work |
|---|---|---|
| Native content | Offline converters, versioned packs, stable IDs, manifests/provenance; runtime loads native data | Complete per-entry behavior and fidelity census; regenerate defective conversions |
| UI | Original art and cached fonts; native menus/options/HUD/selectors/wrench/events/avatar/save/load; input models | Complete normal workflows and disabled adapters; subjective fidelity; display follow-up |
| Building | Selection/favorites, ghost placement, core tools/prints/properties, subset events, save/load; normal App tests | Exact re-centering/reach/timings, macros/undo breadth, special stock brick behaviors, full events |
| Player | Original avatar/animation/customization; custom fixed 120 Hz motor over Rapier, both views | Reported crash, movement/jet feel, prediction/interpolation integration, all player types/emotes/death |
| Maps | All 14 reference missions listed/loadable; architecture/textures/collision/environment adapters | Full sky/fog/decorations/water/snow/detail/shadows/material fidelity, special Tutorial/Storm behavior, streaming |
| Audio | Opus converter/runtime integrated; original resources; menu/core cues tested silently | All gameplay bindings/loops/music bricks/vehicles and listening acceptance; no invented title music |
| Weapons/items | Converted assets, inventory/equip/item presentation/animation/FX foundations and partial authority | Every vanilla weapon/item/projectile complete through UI, authority, damage, audio and effects |
| Vehicles | Converted/native prototype modules and research | Complete playable Jeep/Tank/Carpet/etc., mounting/seats/drive/weapons/damage/respawn |
| Minigames | Audit and partial/drafted UI/models | Actual complete authority/membership/settings/loadouts/scoring/permissions/reset/death/events; menus alone are not done |
| Networking | Authoritative QUIC, native identity, host/direct-IP, building/chat/late join; loopback tests | Prediction/interpolation, discovery/trust UX, persistent host certs, join passwords, broader performance |
| Administration | Host/admin roles and native authenticated identity; persistent bans; client adapter/UI | Complete original Admin/SuperAdmin/trust workflows and source-backed role fidelity |

Runtime packs at the delivered baseline (`ContentConfig::default` in client
`content.rs`; optional `content/client-content.json` overrides these):

`map-bundle-014`, `stock-catalog-004`, `maps-pass-003`, `effects-pass-004`,
`worlds-pass-004`, `ui-pack-003`, `brick-materials-001`, `avatar-pack-001`,
`effects-runtime-pack-002`, `audio-pack-001`, `weather-pack-001`,
`foliage-pack-001`, `weapons-pack-003`, `item-presentation-pack-003`.

Finite terrain region: `[-64, -64, 384, 384]`. An asset file being present does
not mean every corresponding feature is bound into gameplay.

## 6. Terrain / Opus handoff: do not repeat the mistaken assumption

Opus did NOT deliver a finished CDLOD renderer or runtime streaming integration.
It delivered `content/src/terrain_field.rs`, converter terrain changes and
terrain_bundle tooling, `physics/src/terrain.rs`, and `content/map-bundle-015`.
Data handles height queries, holes, repetition metadata; physics has streamed
heightfield primitives. Those modules are preserved, but App/scene/render/sim/
server wiring and final renders/performance evidence were not delivered.

The playtest deliberately uses the existing finite 014 path, with the exception
disclosed. Do not switch the pack default to 015 and call streaming done. Reconcile
render/collision/query sampling and per-player/object coverage together, test
holes and terrain repetition, then wire authoritative behavior and GPU LOD.
RepeatTerrain defaults for omitted fields remain a source fidelity question.

Only six of the 14 maps contain terrain: Bedroom/Dark, Kitchen/Dark, Slopes,
Tutorial. Slate and other flat environments remain required maps too.

## 7. Full original alpha definition of done

Use every unchecked item in `alpha-contract.md`; this list is a navigation aid,
not a replacement that narrows it:

- All 14 vanilla maps: Bedroom, Bedroom Dark, Kitchen, Kitchen Dark, Slopes,
  Slate, Construct, Destruct, Halloween Slate, Skylands, Slate Desert, Slate Sea
  Revised, Slate Storm Revised and Tutorial. Tutorial IS present; old audit prose
  saying it was missing was corrected in `vanilla-reference.md`.
- Complete vanilla bricks, prints, lights/emitters, tools, building, favorites,
  save/import/export/persistence, special stock behaviors and familiar controls.
- All stock weapons/items/projectiles, not just Gun: Rocket Launcher, Spear,
  Sword, Bow, Akimbo, Horse Ray, Push Broom, keys, skis and verified variants.
- All stock vehicles/mounts, not just Jeep: Tank, Magic Carpet, Flying Wheeled
  Jeep, Horse, Ball, Pirate Cannon, Rowboat and the verified stock census.
- Original player types/customization/animations/emotes; faithful walking,
  jumping, crouching, jets, collisions and cameras; complete effects/audio.
- Every vanilla wrench event and target class, including minigame/player/client/
  vehicle/projectile/bot dependencies. Inventory counts are not implementation
  counts (audit found 9 core inputs, 65 core outputs plus add-on registrations).
- Event quality-of-life: more than 100 rows, ordered zero-delay chains/relays
  without an imposed 33ms per hop, bounded fair execution, clear overload/loop
  diagnostics and cancellation. Do not silently drop authored behavior.
- Functional minigames including creation/settings/membership/invites/loadouts/
  scoring/damage/reset/end/death/respawn and associated events. Do not invent a
  vanilla configurable lives limit; audit found unlimited stock lives.
- Complete host/player/chat/trust/Admin/SuperAdmin workflows, authoritative
  permissions, direct-IP/LAN multiplayer and late join, no legacy numeric-ID trust.
- Named fidelity gaps stay requirements: sky/fog, decorations, water/snow,
  terrain detail/streaming, brick surfaces/prints/color and shape FX, legacy
  color sentinels, lighting/shadows. Parse success does not close these.
- Measured performance and eight-client independent-area bot/event workloads;
  transparent operating envelope, not promises of unlimited events/bots.
- Packaged playable Windows build, reference worlds, logs, instructions, known
  issues, verified source push; portable architecture with only actually tested
  platforms claimed. Maxwell judges interactive feel and sound.

## 8. Verification baseline and commands

Before initial release: client library64 passed (19 asset tests ignored in that
invocation), transport1; explicitly run native App flow2, item offscreen1,
native host identity/admin1; admin adapter5/UI6; network loopback12 (2 ignored).
Strict all-target Clippy passed for client/content/physics/converter. Windows
offscreen evidence used NVIDIA RTX 4070 SUPER/Vulkan. These are checkpoint
results, not a blanket assertion that current concurrent edits pass.

Relevant commands (native-content tests intentionally opt in):

```powershell
cargo test -p bri-client --test movement_crash -- --ignored --nocapture
cargo test -p bri-client --test app_flow -- --ignored --nocapture
cargo test -p bri-client --test app_item_render -- --ignored --nocapture
cargo test -p bri-client --lib native_weapon_catalog_startup_and_headless_host -- --ignored --nocapture
cargo test -p bri-net --test loopback
cargo test -p bri-client --lib --test transport
cargo clippy -p bri-client -p bri-content -p bri-physics -p bri-convert --all-targets -- -D warnings
.\tools\tests\Test-PlaytestPackaging.ps1
.\tools\tests\Test-PlaytestLauncher.ps1
```

Rebuild AFTER final code integration; never package the outdated ordinary
`target/release/bri-client.exe` by accident:

```powershell
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --release --locked --target x86_64-pc-windows-msvc -p bri-client --bin bri-client
$releaseExe = (Resolve-Path target/x86_64-pc-windows-msvc/release/bri-client.exe).Path
$releaseHash = (Get-FileHash -LiteralPath $releaseExe -Algorithm SHA256).Hash
.\tools\package_playtest.ps1 -ExecutablePath $releaseExe -Version 2026-09-27-02 -ExpectedExecutableSha256 $releaseHash
.\tools\package_playtest.ps1 -VerifyPackage .\dist\BlocklandReImagined-building-playtest-2026-09-27-02
```

Use a fresh version if 02 already exists. The packager refuses overwrites. Run
the packaged executable with `--check <packaged-content> <isolated-state>` only;
`--run` is Maxwell's interactive path. Verify archive payloads if making a ZIP.
Preserve private user state and leave the original release/tag reproducible.

## 9. Recommended next steps for the receiving agent

1. Coordinate with the active movement/prediction agent. Inspect the current
   diff; identify ownership and do not package a half-integrated API rewrite.
2. Finish the animation regression and corrected logging. Reproduce the old
   failure and demonstrate the intended combinations now pass. If jet-only
   remains unproven, gather the actual error using the fixed launcher with
   Maxwell performing the input. Do not claim all movement crashes solved.
3. Review/integrate the movement agent's work and separate motor tuning from
   network response/camera/animation issues. `player-simulation.md` distinguishes
   sourced constants from assumptions; do not tune blindly or import Blockland2
   scale changes. Compare against v20 with Maxwell's feedback.
4. Run relevant headless App/network/render checks, build a new patch package,
   verify it, give Maxwell the exact replacement launch path, and commit/push
   reviewed source. Keep package version and source revision explicit.
5. Close core building playtest blockers before reopening big systems. Then
   work through full vanilla coverage using per-entry source → conversion →
   behavior → UI/network integration → test evidence. Require no ceremonial
   re-approval of already authorized work, but preserve scope and testing rules.

## 10. Source map / research to reuse

- Intent/acceptance: `creator-direction.md`, `alpha-contract.md`,
  `playtest-contract.md`, `vanilla-coverage.md`, `event-modernization.md`.
- Inventory/maps: `vanilla-reference.md`, `vanilla-reference-inventory.json`,
  `vanilla-inventory.json`; converters under `crates/convert` and `*-import`.
- Integration: `crates/client/src/app.rs`, `content.rs`, `platform.rs`,
  `network.rs`, `avatar.rs`, `building.rs`, `audio.rs`, `admin_ui.rs`.
- State/authority: `crates/world`, `sim`, `net`, `admin`, `identity`.
- Rendering/content: `crates/render`, `content`, `fx-runtime`, `weather`,
  `foliage`; gameplay adapters in `weapons`, `vehicles`, `audio`, `ui`.
- Movement: `player-simulation.md`, `crates/sim/src/player.rs`, `prediction.rs`.
- Detailed handoffs: `runtime-*.md`, `native-client.md`, `networking.md`,
  `ui-handoff-status.md`, and `research/` subfolders for UI/UX, audio, admin,
  minigames, vehicles, weapons, item/weapon FX, foliage, weather and events.
- Opus UI audit has source citations, controls/layouts, stock defaults and
  fidelity questions. Some original rendered/transcribed research is local and
  ignored intentionally. Read corrected scope/reference docs before adopting
  historical audit assumptions (e.g. missing Tutorial, old LAN trust).
- Ignored evidence: `artifacts/native-client-flow`, `native-world-items`,
  `native-ui`, `native-audio`, reference audits and other probe reports.
- `PLAYTEST.md` and `KNOWN-ISSUES.md` describe the initial user-facing package.

Do not restart this project from scratch. The hard part now is coherent runtime
integration and faithful behavior, with concrete tests and Maxwell's feedback.
