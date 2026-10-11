# 2026-10-10 P90 material correction from Maxwell's playtest

Maxwell's screenshot showed only the tan magazine where the P90 should be.
Inspection found all eight P90 materials, its magazine and shell marked
additive by the release importer. Black additive surfaces contribute no light,
consistent with the missing black gun body. The geometry is present: four gun
objects, with 1,860 body and 522 scope vertices alone.

The private port recipe now overrides these weapon surfaces to ordinary opaque
v20 decal surfaces. Runtime already implements translucent texture alpha as
pigment coverage for this material mode. It also resolves the red sight,
black25 accents, yellow shell and black.001 shell material against the designated
read-only vanilla reference instead of clear placeholders. Original images are
copied unchanged into the private native output; model hashes are regenerated.
Added explicit `--vanilla` input to the recipe. No engine update is required.

Rebuilt into `E:\Downloads\BRI-P90-native-v028-material-fix`, updated both
installed native folders, and refreshed the reusable ZIP. Released client
`--check` with Maxwell's now-enabled P90 passed, exit 0, and reported no P90
presentation faults. Existing Butterfly Knife warnings persist. No interactive
gameplay or window was automated. Maxwell must restart the game to reload the
model and verify the corrected appearance.

Earlier worktree-target cleanup was rejected by command policy; its build
output remains. The main checkout's build output was never touched.
