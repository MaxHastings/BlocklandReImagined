# 2026-10-01 The Fill Can stays in hand when a colour is picked

Max, in the d6152ab58 test build: pressing E with the Fill Can out gave him
the plain spray can, so he could not pick its colour.

- Cause: entering the paint box from the tool box made our client send
  `UnUseTool` before the pick (it kept the tool only for images that take
  the cans with `commands.paint`). The host then saw no Fill Can in hand at
  `UseSprayCan`, so its `paint_picker` remount (v20's packaged
  `serverCmdUseSprayCan`) never fired. The v20 client never put the tool
  away there, which is why that package worked.
- Client: `ImageKeys::paint_picker` from the held image; the HUD keeps the
  tool when the paint box opens (`keeps_tool_for_paint`), and a pick keeps
  the tool as the client's equipment while still sending `UseSprayCan` or
  `UseFxCan`.
- Host: a picker taken from the tool box goes back as that tool
  (`equip(slot)`), so its slot stays selected after the pick instead of
  being cleared by the can's `mount_image`.

Tests: `bri-client --lib picking_a_can_keeps_a_paint_picker_in_hand` and
`bri-sim --test fill_can picking_a_can_puts_a_paint_picker_back_as_the_selected_tool`
(a plain tool still gives way to the can).

Separate, not fixed here: the test build lacked every port's `-rules`
companion (the bundle tool, Bundled originals lane), so the Fill Can could
not fill or answer `/fillcan` there.
