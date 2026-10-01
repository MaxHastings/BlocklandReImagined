# 2026-10-01 Change Map shows each map's picture

Max saw "UNKNOWN MAP" in the admin Change Map window for Kitchen on the
v0.1.11 test build (main d6152ab58). It happened for every map.

Cause: `AdminScreen::refresh` for `changeMapGui` filled the list, the name
and the description, but never set the preview bitmap. The control kept
its authored image, v20's `base/data/missions/default` ("UNKNOWN MAP"). The
pictures themselves were fine: every content map carries an
`IconRef::Pack` preview (content test in `crates/client/src/content.rs`),
and Start Game already showed them.

Fix:
- `name_map_preview` finds changeMapGui's preview as the bitmap authored
  with the default map picture and names it `NativeMapPreview`.
- `refresh` shows the picked map's preview from this client's own map
  catalog (`core.maps`, same ids the host lists). A map without one, or no
  pick, falls back to the authored placeholder, like v20's
  `getMissionPreviewImage`.
- `View::set_icon` is now the one place that shows an `IconRef` in a
  bitmap; Start Game, Avatar, Save Bricks and Load Bricks use it instead of
  their own copies.

Guard: `change_map_shows_the_picked_maps_picture` in
`crates/ui/tests/admin_screens.rs` clicks Kitchen, then a map without a
picture. Synthetic variant on the fixture changeMapGui (now with its
preview bitmap); content twin on `ui-pack-004`. Fails on the old code
(`left: None, right: Some("fixture/maps/kitchen")`).

The faint "Administrator Menu" behind the Change Map title bar: v20's
adminGui stays open under changeMapGui (its own `canvas.pushDialog`), and
window skins are drawn with their image's alpha, as Torque does. Left as is
(inferred from the skin path; not measured).
