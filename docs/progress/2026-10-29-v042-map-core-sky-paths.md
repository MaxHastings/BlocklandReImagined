# 2026-10-29 Map core paths and installed vanilla sky reuse

Maxwell explicitly included Map_IZB's path failure in the v0.4.2 importer fix
after confirming the named sky exists in vanilla. Coordinated shared paths
and finite acceptance with the merge chat. Zombie and Roll gameplay ports
remain deferred. This supersedes the unresolved path status in the earlier
importer compatibility note; compile/platform/integrated checks remain queued.

Evidence: Map_IZB's original mission names
`~/data/skies/Sky_Blue2/resource.dml`. Its ZIP lacks the sky, but the designated
vanilla reference has `base/data/skies/Sky_Blue2/resource.dml` and all seven
images. The installed v0.4.1 native map bundle already carries their original
bytes, source identities and hashes in its Environment entries. Existing
`convert::water::resource` already implements legacy `~/data/` as `base/data/`
while `~/Map_*` remains in Add-Ons. Mission/environment conversion differed.

Changes in the offline conversion seam:

- Central mission asset-path resolution handles the legacy core-data prefix,
  retains relative/mod-root references and rejects traversal. Sky conversion
  and terrain-instance texture resolution use that rule; the existing
  `~/Map_Slate/slate.dif` control remains intact.
- Automatic map conversion passes the installed map-bundle role through an
  optional offline `--sky-bundle` argument. No runtime dependency on Torque
  readers, new root manifest/lockfile dependency or native schema is added.
- Only when the exact original DML is unavailable, conversion can use its
  exact matching source-material entry in that installed bundle. Original
  DML SHA, face/reflection/cloud ordering and original image bytes are retained;
  no synthetic DML, generic replacement sky or global file search is used.
  Multiple entries must agree on resource identity. Native Environment budgets,
  confined reads, image hashes and actual dimensions are checked before writes.
- Fog, solid colour, texture/bottom flags and cloud motion/height come from
  the incoming mission, not the stock map that supplied image resources.
  Invalid installed package directories propagate errors. Core data paths
  are no longer reported as a borrowed Add-On named `data`.

Added synthetic checks for original-absent installed-sky conversion, incoming
mission policy and byte preservation; refusal of unrelated materials, corrupt
hashes, incorrect dimensions, traversal and conflicting identities; and an
actual importer CLI map conversion with only installed content. The CLI test
removes its authored original source before importing into the content root.

Bounded diagnostic probe, using the existing v0.4.1 importer: copied the map
ZIP to a temporary derived ZIP and changed only mission `~/data/` references
to `base/data/`, then supplied the read-only original reference. Conversion
exited 0, preserving the map's pre-existing retained FileObject warning.
Evidence: `C:\Users\Maxwell\AppData\Local\Temp\bri-map-path-evidence-j_nsl6wx`.
This isolates the path failure; it is not a test of the newly compiled code
or automatic installed-sky path. The original Archive.zip/install is unchanged.

`rustfmt --edition 2024` on changed Rust sources and `git diff --check` pass.
Cargo, platform CI, actual unmodified-map conversion, packaged startup and
main integration are pending root's warm broker after playtest clearance.
No interactive playtest or visual-fidelity acceptance is claimed.
