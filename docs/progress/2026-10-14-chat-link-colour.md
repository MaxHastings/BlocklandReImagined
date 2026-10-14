# 2026-10-14 Chat links, mouse tip and outline clipping

Max reported three chat readability problems: hyperlinks drawn dark navy,
the "TIP: Press M ..." line crowding the SAY: input, and the bottoms of
`g` and `y` looking cut off.

## Link colour
- v20's `BlockChatTextProfile` (allClientScripts c:20286-20304) sets no
  `fontColorLink`, but it sets `fontColors[4]` = 0 255 255 and
  `fontColors[5]` = 255 0 255. In Torque `fontColorLink`/`fontColorLinkHL`
  are field aliases of `fontColors[ColorUser0/1]` (`guiTypes.cpp`
  `initPersistFields`), and `GuiMLTextCtrl::reflow` reads link colours
  from those slots. So v20 chat links were cyan, highlighted magenta.
- We always used `GuiDefaultProfile`'s 0 0 204. `View::ml_layout` now takes
  palette slots 4/5 (`ml::profile_link`), falling back to that default.
  The importer folds `fontColorLink[HL]` into slots 4/5, the nearest
  profile in the parent chain naming either alias deciding each slot.
  No v20 profile sets both names. Chat already had the cyan in slot 4 in
  existing packs, so the colour changes without regenerating content.

## Mouse tip
- v20 puts MouseToolTip one tip height (18) under the chat and the input
  box at the chat's bottom, `font line height + 4` tall, so from mid chat
  sizes up the box covers the tip. While the input is open
  (`Core::chat_input_open`, kept by `Ui::flush`) the tip now goes under the
  box (`play::mouse_tip_rect`). Checked in `tests/chat_links.rs` for chat
  sizes 0, 4 and 10. Reverting the gap to 18 makes that test fail.

## Outline clipping
- Palatino descenders reach exactly the font's line height (ui-pack-004:
  size 18 `g`/`y` bottom 18, size 24 bottom 24), and the 1-pixel outline
  goes one past. Chat and the tip are exactly as tall as their text, so the
  control clip cut the outline under the last line. Controls with an
  outlined font now clip 1 pixel wider on each side.
- Not verified on screen: the container has no GPU adapter. Max should
  confirm descenders on the last chat line and the tip look whole.

## Commands
- `cargo test --no-fail-fast -p bri-ui -p bri-ui-import`: everything passes
  except tests needing a wgpu adapter ("no wgpu adapter"), which also fail
  without this change here.
- `cargo clippy -p bri-ui -p bri-ui-import --all-targets -- -D warnings`.
