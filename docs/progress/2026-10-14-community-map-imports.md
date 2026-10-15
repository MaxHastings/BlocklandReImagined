# Community Blockland maps through `bri-import-addon map`

Maxwell asked for his own and community map Add-Ons to be tried for real:
Map_ArcticChallenge3, Map_AvP, Map_EmeraldIsles4, map_paradise (GSF
Paradise, day and night), Map_Creek and Map_Forest (day and night). All six
now import and pass `bri-addon-check` (staged copies of his v20 install,
read only, nothing committed). Offscreen renders of every map were checked.

## What real maps needed

- Mission object names may be any tokens up to `)` (FileObjects named by
  path).
- DTS mesh kind 3 (TSSortedMesh, tree leaves) is read; its clusters are
  skipped and it draws as one frame (map trees store 0 vertices per frame).
- DIF files from community exporters end with extension word 1 and an
  all-zero block; accepted only when the rest is zero.
- Terrain `bumpTexture = "."` means no texture.
- A sky without `materialList` and with `useSkyTextures = 0` is a solid
  colour sky.
- Original files are found ignoring case (Blockland ran on Windows).
- Water textures named `./water` resolve beside the mission; a reflection
  map from another game falls back to the sky's reflection, with a warning,
  only when the sky has one.
- The plain Import (Add-Ons screen, `tools/addon_bundle.py`) hands a
  `Map_*` Add-On with a mission to the map converter, as package
  `map_<name>` with an import report.
- `bri-import-addon map` copies Add-Ons a map borrows (named in its .mis or
  .ter: Map_Slate buildings, Map_Slopes and Map_Bedroom terrain textures)
  from v20's Add-Ons or beside the map.
- A terrain texture a map never shipped (Paradise's `sand7`) gets a grey
  stand-in under `--stand-in-missing-terrain-textures`, listed in the
  bundle's `stand_in_textures`.
- Every mission of an Add-On becomes one map, each its own bundle in
  `map/<id>/` (a map's bundle.json lists one map).
- fxShapeReplicator is expanded at conversion (`bri_convert::replicator`)
  into static models on the terrain from its seed with Torque's MRandomLCG
  sequence, refusing water and steep slopes; the original's
  copy-vs-copy and interior collision tests are not reproduced.
- Static DTS materials with additive blend render additively.

## Open

- AvP renders very dark: its sun and ambient are black and its interior
  lightmaps average about 12/255 (red). Brightened ×8 the geometry and red
  lighting are right; whether Blockland brightened these lightmaps is
  unverified.
- Not imported: AudioEmitter (Creek's ambient sound), ParticleEmitterNode
  (Creek's waterfall), fxGrassReplicator/fxFoliageReplicator rendering on
  imported maps, triggers and scripts (Arctic's checkpoints: a Rhai rule).
- Bundling these maps with the game goes through the release thread
  (`tools/addon_bundle.py`); Maxwell has the authors' permission.

## Commands

    bri-import-addon map Map_X.zip OUT --v20 "<Blockland v20>"
    bri-addon-check OUT
    scene_snapshot OUT/map ns:map/id out.png [yaw pitch eye-x eye-y eye-z]
