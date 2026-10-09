# 2026-10-09 Gravity Gun gets its own model

The Gravity Gun tool now holds `models/gravity-gun.shape.json` instead of the
Printer's `printGun.dts`. Concept art (dark chunky shell, cyan seams, glowing
orb, claw, yellow grip) was resolved by judgement where its views disagreed:
four claw prongs, simplified seam lines.

- Source: `packages/showcase/gravity-gun-tool/model-source/` (`geom.py` geometry,
  `export.py` writes the model and four flat PNGs, `render.py` previews in
  Blender). Cyan and purple materials are unlit (self-glowing).
- Frame: x right, y up, -z forward; `mountPoint` at the grip's hold point,
  `muzzlePoint` at the front of the orb. `SCALE` in `export.py` sets size.
- `looks.json` no longer applies the alien skin (it was shaped for the Printer
  and would paint over the new model); `skins/alien.wgsl` is kept for retuning.
- Checked: `bri-addon-check packages/showcase/gravity-gun-tool` passes. Not
  checked: hand fit and size in play (printer dimensions unavailable here),
  tests, effects and sounds. Maxwell playtests.
