> **Historical note. Written 2026-09-29 (the file's date in the cloud project folder, `cleanup/cleanup-pass-2026-09-29.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Stability and cleanup pass (2026-09-29)

Branch `claude/project-thread-3pq3wl`, head `afdd43483` (current main merged in).
Checks run in the cloud: `cargo clippy --workspace --all-targets --locked -D warnings`
clean; lib tests for net, ui, sim, convert and client; all tests for weapons, world,
events, admin, client-sandbox, net, stresslab, minigames, package, fx-runtime and bls
pass (content-dependent ones need the PC gate); `gate.py --history-range` ok.

## Removed

### Dependencies
- `bytes` (bri-net), `bri-admin` (bri-stresslab), `serde` (bri-fx-import): unused (cargo-machete).
- Fifteen shared crates are now pinned once in `[workspace.dependencies]`. bri-foliage-import
  dropped its private zip 2 copy. Fourteen crates now inherit version/edition/rust-version/publish.

### Old-format and old-protocol compatibility
- Weapon saves: accepted schemas 1 to 3 with old drop limits; now only `SAVE_SCHEMA` (3).
  Drop timings are named constants. `Frame.body_yaw`, `Drop.rotation` and `Drop.scale` no longer default.
- `WrenchProperties.item_spawn`: a wire-only default.
- Event checkpoint: removed an origin cursor that was written and never read, plus three
  old-checkpoint defaults. The checkpoint schema is now 2.
- `events::migration::legacy_event` and `ui_catalog`: had no callers. The live conversions
  moved to `events::convert`.
- Wire fields from protocols 39 to 50 (accept_differences, projectile_falls, hide_bar,
  wheel_contact, vehicle look and steering inputs) no longer default.
- The protocol changelog for versions 34 to 52 is gone from protocol.rs (git has it).

### Dead code
- `bri_admin::view` (an unused second admin-screen model) and `Role::badge`.
- net: `start_with_admin_store`, `codec::read_request`.
- ui: `geom::mul`, `with_alpha`, `Gpu::remove_external`, `text::ml_runs`, `chord_label`, `unbind_input`.
- sim: `event_datablocks`, `package_vehicles`, `packages_enabled`.
- `convert::terrain::resolve_packaged`, `fx Pack::bindings_for`, `image_scene`, `RELEASES_PAGE`.
- The always-false "legacy LAN" argument of `events::semantics::minigame_target`.
- `bri_physics::preflight` plus `physics_probe`, and render's `gpu_probe` plus `probe.wgsl`: early
  library spikes.
- `regenerate_bricks`: a one-off pack migration.
- The macOS branch of the client's default state folder.

### Tools
- `tools/inventory_v20.py`: nothing calls it.
- `tools/verify_terrain_conversion.py`: reads a terrain manifest that no current pack has.
- `tools/convert_stock_catalog.ps1`: `regenerate_content.py` runs the same converter.
- `tools/package_playtest.sh` and `launch_playtest.sh`: Linux forks of the Windows packager that had
  drifted (no Add-Ons, Duplicator, Stress Lab or verification).

### Lint, stability and tests
- An `#[allow(unreachable_code)]` in the net hardening test.
- The client sandbox's `check_path` now uses `bri_package::path::inside`. This resolves its `TODO(PR #1)`.
- Two weapons tests that need converted content are `#[ignore]`d, so GitHub CI now runs the
  content-free weapons tests instead of skipping the whole target.

### Docs
About 35 files. The main fixes:
- README ("exe is coming"), TESTER-GUIDE (exe state and log paths).
- PLAYTEST de-duplicated against TESTER-GUIDE.
- AGENTS.md: pointed at STATUS.md and dropped the feature-freeze line, which contradicted STATUS.md.
- playtest-contract.md and stress-lab/HANDOFF.md marked as history.
- Architecture docs updated: statuses, and the join and Add-On mismatch flow.
- networking.md, native-client.md, playtest-package-layout.md, content-regeneration.md.
- Crate READMEs: pack numbers, and the `--target-dir` flags AGENTS.md forbids.
- Broken links fixed.

## Kept on purpose
- The schema 5 to 6 vehicle pack upgrade (restored after Gate's content check): every generated
  vehicles pack through vehicles-pack-011 is schema 5. Delete it once the pack is regenerated.
- Defaults on `Brick.item_spawn`, the effects pack's `composites` and the UI pack's
  `event_tables`/`avatar`/`help`: generated packs and saved builds may predate them.
- `JoinBegin.purpose` default: lets a peer on another version decode far enough to be told which
  version to run.
- Client `Settings` missing-field defaults: players keep their settings across builds. Without them,
  every build that adds a setting would show "your settings could not be read".
- `AutoRole.name` default in `administration.json`: added today. Dropping it would make a host's
  saved ban and rank list fail to load on update.
- Launcher carry-over of player files on update: this is normal updater behaviour, not format migration.
- `packages.rs` "stale saved keys" tolerance: this is about Add-On authors changing their own state
  keys, not our format.

## Handed to Gate
- **CI on main has been red for at least three runs** (`bri-vehicles` flying_vehicle, impacts and skis
  need generated content and are neither `#[ignore]`d nor ci-skipped). Two fixes were blocked by the
  permission classifier as a CI bypass: adding them to `ci_skip_target`, and bumping
  `actions/checkout` / `upload-artifact` from v4 to v5 (the Node 20 deprecation warning). Gate is
  making both fixes on the PC (gating the three vehicle test files on content being present), so
  nothing waits on Max.
- Protocol 53 (brick load lane) will conflict on the protocol.rs header comment. Resolution: keep their
  `VERSION = 53` and drop their changelog line.

## Left for lanes (their areas)
- net `server.rs:774/799` `#[allow(unreachable_code)]`: move the loops into small async fns
  (brick load lane edits server.rs). The ~35 `too_many_arguments` allows are mostly in client, render
  and net; the `connect_*` family could take a `JoinRequest` struct.
- `AdminSnapshot.legacy_lan` is always false but crosses the wire. Remove it with the next protocol
  number.
- `weapon_debris::pending_count` is unused (entity perf lane).
- `ui::binds::Platform::MacOs` / `Linux` evaluate v20 default binds for other platforms. The game is
  Windows only (bug sweep UI area).

## Possible fidelity gaps: v20 rules written but never wired in (not deleted)
- `events::semantics`: `touch_input_allowed` (no onTouch while holding the admin wand),
  `brick_spawn_allowed`, `sound_allowed`, `may_recover_vehicle`, `item_spawn_position`, `item_yaw`.
- `minigames::moderate_end`: host or admin ends a minigame; nothing calls it.
- `client::building::map_blocks`: the GuiShapeNameHud sight test.
- `ui::models::hud::brick_icon_tint` / `paint_label_color`: RecolorBrickIcons.
- `upnp::PortMapping::behind_another_router`: the double-NAT hint is never shown.
- `client-sandbox` `revoke_server` / `revoke_addon`: no UI can revoke trust.

## Engine seams (north star: no game code in engine crates)
- `crates/net` depends on sim, weapons, vehicles, minigames, admin and events. `stream.rs` WeaponStream
  and `content_identity.rs` hold weapon-specific logic. Suggest splitting transport from game server.
- `protocol.rs` keeps adding top-level game fields (`WeaponDelta`, `broken_shapes`,
  `projectile_falls`). New concepts should go through `entities` / `EntityDelta`.
- `package-runtime/src/content.rs:370` hard-codes the Slate map as the default environment. It
  should come from the base package manifest.
- `package-runtime` `PlayerView.minigame` and the sandbox's dedicated vehicle host calls put vanilla
  concepts in the mod ABI.
- `crates/content` holds `tutorial.rs` and `avatar.rs`, which are game schemas.
- `platform-door-closers.md` §3, §5 and §10/§12 describe problems already fixed. Its line numbers
  have drifted.
- `servers.json`, `trusted-hosts.json` and `trust-list.json` have no `schema_version`. A bad trust list
  silently empties.

## Tooling notes
- `tools/tests/*.ps1` (packager and launcher tests) are not run by CI or the gate.
- `docs/research/item-rendering/build_presentation.py` and `docs/research/weapon-*/importer` are
  build inputs that live under `docs/research`.
- There is no `rust-toolchain.toml`. The 1.93 pin lives in Cargo.toml and both workflows.
