# bri-ui — native interface integration

Both `bri-ui` and offline `bri-ui-import` are members of the root workspace.
The implementation is under `src/`.

## Implemented interface layer

The original skins, cached fonts and authored layouts drive the menu/dialog
stack. Native screens now implement main/start/join/direct-IP/loading/pause,
options/remapping, runtime HUD/chat, brick/cart/favorites, prints, wrench variants,
events, avatar/palette, save/load and player-list interactions. UI actions are
typed requests with acknowledgments and rejection handling. No scripts execute.

The `bri-client` host now connects map loading, authoritative movement/chat,
networking, scene/UI compositing, display requests and settings persistence.
See [native client integration](../../docs/native-client.md) for verified limits.
Screens for minigames, trust, admin, server config, Add-Ons and music live in
`src/screens/`. Disabled/development notices do not satisfy the full vanilla
contract. See `docs/alpha-contract.md` and `docs/STATUS.md`.

## Host contract

1. Load the converted `Pack`, create `Ui` with physical window size, platform and
   saved `Settings`. `Settings::default()` opens the hardware/default-controls screen.
2. Supply verified runtime catalogs through `UiUpdate`. Do not expose every
   installed map/add-on as vanilla merely because it appears in the UI pack.
3. Feed platform-neutral `InputEvent`s in physical pixels; `update(dt_ms)` advances
   repeat/animation timers. Use `cursor_visible()` to control host pointer capture.
   Focus loss and modal dialogs release held game controls and cancel repeats.
4. Drain `(RequestId, UiAction)` requests. Preserve IDs through the host command
   dispatcher and return `ActionResult` for pending requests. UI choices are not
   authority: the server validates permissions, IDs, parameters and file operations.
5. For each HostGame/JoinServer/StartTutorial request, capture its RequestId as the
   connection token. Deliver **all asynchronous session updates** through
   `apply_session(token, update)`. Cancel/disconnect/new attempts invalidate stale
   responses. `apply()` is for synchronous local data or already-checked updates.
   The token is local lifecycle bookkeeping, never a network authority credential.
6. Send Tools/BrickInventory before authoritative active-slot updates. The latter
   update visuals only and never echo a new equip request. Invalid/empty slots
   clear selection. Native user input still emits the appropriate requests.
7. Render `ui.draw()` with `UiRenderer` to a non-sRGB color target (or perform the
   correct display-space composition in the host). All geometry uses logical
   pixels; apply `ui.scale()`. `TexKey::External` source rectangles are normalized
   UVs; other textures use pixel rectangles. Register host preview/icon textures
   with `set_external` before rendering.
8. `PreviewAvatar` is a nonpersisting local render request; return AvatarPreview.
   `SetAvatar` is an acknowledged authoritative change. Keep preview failure as
   unavailable state rather than generating a popup for every drag frame.

Event catalogs support class-qualified capabilities through
`EventCatalog::from_capabilities`. The CURRENT_BRICK_EVENT_* constants describe
today's limited engine adapter, not the alpha definition of done. Full vanilla
Player/Client/MiniGame/projectile targets remain required. Preserved imported
rows are opaque; event copy never transfers their tokens between bricks.

## Working UX decisions

Remap replaces the previous binding; Linux uses Windows defaults. Modal dialogs
and text entry suppress gameplay while releases still reach the controller.
These are deliberate quality-of-life defaults under Maxwell's pivot authorization.
Preserve them in settings/behavior documentation for playtest feedback.

## Commands (workspace root; no visible window)

```
cargo test -p bri-ui -p bri-ui-import
cargo run -p bri-ui --bin ui_runtime_probe -- content/ui-pack-003 artifacts/native-ui-runtime
cargo run -p bri-ui-import -- --v20 "<v20-install>" --decompiled .research/v20-dso --stock-defaults .research/bl-decompiled/v20/client/defaults.cs --brick-catalog content/stock-catalog-004/stock-catalog.json --out content/ui-pack-NEW
```

Output directories must be new and outside the original install. Canonical003
includes all166 selectable stock brick icons. Original content remains ignored.
Converted-content GPU checks are explicit ignored tests; default tests use
synthetic data and operate on the UI event model, never desktop input.
