# Native sky, clouds and distance fog

The converter copies six original sky faces, the separate reflection texture,
and up to three cloud layers
byte-for-byte into a native map bundle. Versioned metadata retains original DML
and image hashes, fog range/color, texture/bottom/horizon flags, cloud heights and
wind-derived UV velocity. Original formats remain outside the runtime graph.
The runtime verifies image hashes/dimensions and bounds compressed reads and total
declared sky image memory before uploading textures.

The current client uses `content/map-bundle-014`, covering all 14 reference maps.
Primary geometry and resources come from the new E: reference via maps-pass-006.
Mission lighting is baked from the reference originals by the converter; see
content-conversion.md. Remaining lighting gaps are listed there.

Environment schema 2 corrects a schema-1 conversion error: DML slot 6 (zero based)
is a reflection map; cloud layers start at slot 7. The first pack mistakenly drew
reflection textures as clouds, producing oversized moon shapes. Schema 1 is
rejected; pack 009 supersedes that evidence. The pinned engine
[sky.h](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/terrain/sky.h)
defines these offsets. The current reference has three moving layers only in
Slate Storm Revised; the other skies have no separate cloud layers. Reflection
images are preserved for later material/water binding. Destruct deliberately
has no material list and draws its black fog backdrop without replacement art.

## Adaptation

Background meshes stay centered on the camera, retain native face orientation,
draw at far depth without writing depth, and precede world geometry. Cloud UVs
advance from the camera time uniform without rebuilding/uploading map geometry.
Horizon backing/bands cover below-horizon rays, preserving output color transfer
on both sRGB and UNORM attachments. World surfaces and terrain blend toward the
authored fog color using distance and the classic nonlinear haze curve.

Engine-family evidence is pinned OpenMBG commit
`9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7`, specifically
[sky.cc](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/terrain/sky.cc)
and [sceneState.cc](https://github.com/MBU-Team/OpenMBG/blob/9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7/engine/source/sceneGraph/sceneState.cc).
This supports face ordering, camera-relative placement, cloud grids/alpha/time
and haze shape. It is not proof of exact Blockland binary behavior. In particular,
the classic rendering path translates to the camera but does not apply the Sky
object's authored rotation; the current adapter follows that behavior.

## Evidence and remaining acceptance

`persistent_scene` tests verify six face directions, absence of sky translation
parallax, foreground depth, numeric haze at the midpoint, cloud motion with one
GPU upload, calm clouds, UV wrap, and existing output color-transfer behavior.
The 14-map offscreen test passed and writes images/report to
`artifacts/persistent-scene/`. Corrected visual inspection included Slate,
Tutorial and Slate Storm Revised. Converter regression tests distinguish the
reflection slot from clouds and verify original copied bytes and empty skies.
Slate has one collision interior and no visible interior surfaces; the render
test now correctly expects its sky-only architectural view.

This does not close the environment acceptance gate. Active height-fog volumes,
storm transitions, exact horizon/UV visual fidelity, settings integration,
water fidelity/snow, replicated foliage, static-object animation/destruction, dynamic lights/shadows, texture detail/mips,
terrain streaming and each map's special gameplay remain work. The current
finite terrain patch and baked-light cache dependency are still explicit gaps.
The cloud clock currently wraps once per day and is local to the client;
long-session phase continuity and multiplayer environment timing need review.

## Static map objects

Bundle 011 also binds 24 direct native assets and 76 original material textures,
with no unresolved bindings. Thirty TSStatic placements (trees and stove burners)
and 52 StaticShape placements (glass, lamps and LCD clocks) are drawn across
Bedroom/Kitchen and their dark variants. Tree materials use the original parent-
directory texture lookup. The offline converter selects literal StaticShapeData
declarations from the recovered core scripts without executing their callbacks;
the script hash and retained declarations remain provenance in the ignored pack.
Original LCD skin variants and initial time0/blink poses are bound explicitly.

Physics uses authored model collision details, including hidden collision objects;
leaves and burners without authored collision do not become solid obstacles.
A nonuniform placement/raycast test distinguishes visual geometry from authored
collision. The 14-map loader/render checks cover all static placements without
omitted StaticModel/DatablockModel diagnostics. Kitchen visual inspection confirms
the fixtures in their map context. These checks do not yet prove interactive
glass destruction, clock ticking/blinking/explosions, repair, distance LOD or
replicated grass. Those remain explicit diagnostics and alpha work. Translucent
glass still uses mesh-batch-center sorting, with intersecting-surface limits.

Water rendering/host coverage now use the versioned native records in bundle 014;
see `map-water-weather.md` for evidence and the still-open visual/gameplay gaps.

Reproduction (no visible window or OS input):

```powershell
cargo test -p bri-render --locked --test persistent_scene
cargo test -p bri-render --release --locked --test persistent_scene real_native_maps_upload_once_camera_motion -- --ignored --nocapture
cargo test -p bri-client --release --locked local_native_content_index_and_lazy_maps -- --ignored --nocapture
```
