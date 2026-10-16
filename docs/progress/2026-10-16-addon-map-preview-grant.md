# 2026-10-16 Add-On preview boundary and fix grant

The core owner traced actual Creek and Emerald Isles source archives to native
packages: source preview JPGs exist but their bytes and bindings are absent from
the output. ClientContent additionally assigns every Add-On map IconRef::None.
Start Game and AdminMaps both consume MapInfo.preview. Menu probe is still in
progress; do not confuse source/loader inspection with completed UI validation.

Root granted only addon-import/src/v20_map.rs, client/src/content.rs, new
addon-import/tests/map_preview.rs, new client/tests/addon_map_preview.rs and the
owner's progress note, from clean report459970cb on a preserved new branch.
Copy authored validated source bytes into package-local storage with per-map
metadata, then safely register the image through existing client UI mechanisms.
No menu, network, protocol, manifest/dependency or unrelated paths are granted.
Resolve source-qualified relative/case/extension semantics, keep multi-mission
identity, constrain paths and image budgets, and retain missing/corrupt controls.
No fabricated assets or whole-map rejection solely for absent preview.

Baseline/fix targets precede quick/focused changed crates and normal dependents
plus explicit new targets, Cargo2jobs/testthreads8/target unset. Root reviews the
immutable result before integration/reimport of public bundles. AvP remains
excluded from public selection and preserved locally. B4 source remains frozen
in its independent checkout with focused41480 running; these paths do not overlap.
