# 2026-10-03 v0.2.3 Gravity Gun inventory icon orientation

Maxwell requests rotating the Gravity Gun inventory/tool icon 90 degrees
clockwise to match the normal Printer orientation. Root owns shared integration;
this lane prepared `/tmp/bri-v023-gravity-icon.patch` without changing shared
production files or generated/original images. No Cargo/GPU/gameplay was run.

## Actual icon source and proposed correction

The selected Gravity Gun item names `icons/gravity_gun`. Its adjacent authored
`packages/showcase/gravity-gun-tool/assets/icons/gravity_gun.render.json` asks the
existing CPU icon renderer for `pose_like: v20.weapon.printgun`, with the alien
skin from its in-game appearance. `ItemAssets` builds a Request from the player's
actual model/reference/icon, draws it off the load path, caches it by digest and
uses that resulting image in inventory/tool slots. The old static showcase PNG
and generator are not the authoritative render-request path.

The patch adds optional, bounded `clockwise_quarter_turns` (0..3, default 0) to
that existing render specification; the Gravity Gun's authored request chooses
1. The output raster's texel coordinates rotate around its canvas while its
pixels are composed. Model fitting, model orientation, light/normals, textures,
skin and antialiasing are untouched: every RGBA byte is preserved at the rotated
coordinate. Non-square canvases exchange dimensions. Neither the held tool nor
spawned item model changes. This is a runtime render-source change, not a manual
edit of original/icon image assets.

Default zero is omitted when serializing the specification, preserving existing
inputs/cache keys for unrelated icons. Request::digest already includes the
serialized specification, so choosing one produces a different Gravity Gun key
and old cached orientation cannot survive the update. No drawing-version bump
or global cache invalidation is required.

## Proposed verification

A synthetic, asymmetric 72x48 skinned-model regression verifies exact clockwise
RGBA mapping into a 48x72 canvas, default-zero parsing, invalid-turn rejection
and changed cache identity. The existing Gravity Gun model/icon integration test
now asserts the authored turn, rotated Printer framing and exact pixels against
the same current model/reference rendered at zero turns. It still checks skin,
framing, asynchronous drawing and cache reload. Synthetic and ignored native
content variants remain available; generated pictures stay under target/.

Formatting and `git apply --check /tmp/bri-v023-gravity-icon.patch` passed.
Root owns tests under its compute lease:

- `cargo test --locked -p bri-client --lib screen_quarter_turn_rotates_an_icon_clockwise -- --nocapture`
- `cargo test --locked -p bri-client --lib the_gravity_gun_icon_is_drawn_from_its_model_like_the_printers -- --include-ignored --nocapture`

The second produces the existing bounded
`target/gravity-gun-icon-vs-printer.png` for visual review.

Root's focused unit run passed **1/1** (`/tmp/bri-v023-icon-unit.log`). The
actual Gravity Gun/Printer integration passed both synthetic and ignored native
content variants **2/2** (`/tmp/bri-v023-gravity-icon-native.log`). Root viewed
`target/gravity-gun-icon-vs-printer.png` and confirmed the clockwise turn retained
its original art. Maxwell's actual tool-slot orientation acceptance remains open.
