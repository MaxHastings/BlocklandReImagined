# 2026-10-15 Attached particle frames

The rifle render inspection found that source particle groups carried attachment
and sprite-registration semantics which conversion discarded. Correcting the
mount socket alone could not correct their appearance.

Native burst groups now optionally follow the moving attachment frame. Detached
smoke/casings retain world-space motion. The client keeps attached particle tails
after their instantaneous emitter disappears, carrying positions, directions and
motion through source rotation/translation without overwriting group offsets.
Default behavior remains detached. A synthetic integration regression rotates
and translates an attached group while a detached group remains still, then
verifies expiration; it passes.

Animated particles can supply bounded frame layouts: image proportions and a
normalized registration point. The GPU uses those to place each quad; default
layout remains the existing centered square. Culling includes the authored
extent. Validation rejects invalid geometry and layout references to absent
frames. A frame-playback regression confirms layouts change with the selected
texture and default correctly; it passes.

Actual private captures use source textures with authored pivots/aspects and the
ordinary item/avatar/vehicle/effects renderers. Twenty-four rifle stills and ten
vehicle stills were generated with shared depth, including live digits, reload
poses, procedural sway, an occluding wall, turret aim, chassis roll and three
body types in each seat. No game window, interactive input or video was used.
The corrected wall capture fully occludes the flash. This establishes a bounded
render observation, not an original-game fidelity comparison or human acceptance.

Failures caught during inspection: the first new shader used a reserved WGSL
identifier and failed validation; it was renamed and the full capture rerun
successfully. The first vehicle capture passed the wrong resource-root identity;
the corrected root then exposed stale generated material checksums. The private
manifest was regenerated and all ten captures rerun without asset warnings. No
capture with fallback art is accepted as evidence of source appearance.

Evidence resides only in the main checkout's ignored artifacts directory:
attached-burst-regression.log, sprite-layout-regression.log,
rifle-sprite-layout-inspection and warthog-runtime-inspection. Original art,
conversion provenance and the capture program remain private. Physical sprite
scale relative to the original game remains unverified. Windshield/light fidelity,
projected headlights, exact particle blending/orientation/contact physics, audio,
HUD, slopes and interactive controls remain open. The new release-profile private
candidate is building; this entry does not authorize publication or close the
broader release gate.
