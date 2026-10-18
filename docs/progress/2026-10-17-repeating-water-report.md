# 2026-10-17 Slopes repeating-water report

Maxwell supplied a screenshot of exposed terrain in a repeated Slopes region
and asked whether water fails to repeat, possibly on other maps. Screenshot is
preserved privately at primary artifacts/feedback/20261017-slopes-repeat-water.png,
SHA256145a7e1d326f24bf8e82349aad6c1ffb2a3beaaaa5aa29cf7d662c3a17410bdb.
Screenshot chat text is observation, not task instructions. Exact player location,
client revision and repeat index are not supplied.

Read-only source diagnosis confirms an existing render limitation. Original
read-only E:/Downloads/B4v21Launcher/versions/Blockland v20/Add-Ons/Map_Slopes.zip,
slopes.mis, explicitly sets RepeatTerrain=1 on both TerrainBlock and WaterBlock.
The importer preserves repeating water via repeat_period. Content Water.footprint
wraps coordinates indefinitely, and shared submersion queries use it. However,
render/water_scene.rs append101 builds only fixed repeat indices-1..=1 on both
horizontal axes. scene_loader.rs507 appends that geometry to static SceneData.
TerrainScene.visible127 computes copies from camera position/radius instead.
Thus a repeated water volume can continue physically while its rendered surface
runs out beyond the fixed patch. The converter's existing warning atwater.rs176
already says camera-following water rendering/LOD remains required. These paths
are unchanged from publice4d9bb05 through private651.

This code limitation matches the report but does not prove the screenshot's
precise boundary: authored finite footprint gaps and depth-mask details remain
possible at the unspecified location. It potentially affects other waters with
repeat_period, not every finite lake/pool and not every map. No original install,
production code or active playtest was changed; no build or interactive probe
was started while Maxwell is playtesting.

A bounded correction should make authored repeating water visible around the
camera using the existing period/footprint, with finite-water controls and
render-versus-submersion consistency beyond the old3x3 patch. It must preserve
intentional gaps, primary terrain holes/depth masks and camera/reflection passes,
without replacing finite waters with an infinite plane. This entry records the
bug and candidate boundary, not a completed fix or new implementation grant.
