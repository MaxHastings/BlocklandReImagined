# 2026-10-15 v0.3.1 fidelity completion

Maxwell restored the deferred playtest work to scope, prioritizing missing
engine capabilities and bugs over private Add-On polish. The engine release
remains authorized. Halo/Mario assets and source conversion stay private;
Maxwell owns interactive acceptance.

## Engine work in progress

- Restored the separately preserved contact-effect patch and corrected its
  validation, missing-texture handling and global decal/resource bounds.
- Map material kinds now reach authoritative projectile impact selection.
  Decals accept authored textures selected by surface kind, with the existing
  stock disc retained for content that does not declare artwork.
- Authored particles can play original sprite frames, retain seeded tint and
  size variations, and combine independently placed burst groups with bounded
  count/radius ranges. Particle timing is independent of frame partitioning.
- `Image.shell_effect` uses the existing reliable shell cue and actual socket
  to emit authored sprite/composite casings in place of stock mesh debris.
- Fixed mounted-image composites emitting their initial bursts at world
  origin before their attachment was resolved. They now wait for the pose.

## Evidence so far

Before the additional shell/variation work, targeted material-impact tests
passed (4), effects runtime tests passed (19, 3 existing content cases ignored),
and decal unit tests passed (4), including authored texture UV/alpha and the
shared mark budget. The expanded implementation and its new tests still need
the quick/focused checks and Windows CI before landing.

Baseline PR #169 remains separate from this follow-up branch, with its last
Windows job retrying a dependency-download HTTP 502. No release tag or public
v0.3.1 release exists yet.

The expanded targeted checks also pass: multiplicative-surface offscreen
rendering in Original and modern paths (1), effects runtime (20 passed,
3 existing content cases ignored), mounted actor effects (13 passed,
1 existing ignored) and weapon effects (15 passed, 1 existing ignored).
Those tests include seeded group placement/size/tint, independent sprite
frames, sprite ejection and mount bursts born at the resolved pose.

## Remaining work

Finish the private original-effects/audio bindings and verify the preserved
HUD, rifle, Warthog, camera, seating and tool-action changes. Inspect imported
map lighting support and the measured shader-startup delay. Run bounded
offscreen/private checks, integrate all verified engine changes, then run the
full release gate and publish the public engine packages. Do not count visual
acceptance or private asset fidelity complete from source conversion alone.

## Integration and source checks

PR #169 passed every Windows check in run 38712921056 attempt 2 and merged
into v0.3.1-experimental at 6d1c7121. The initial download failure was HTTP
502, not a test failure. The follow-up engine branch remains separate.

The generic particle renderer now packs unchanged RGBA images onto atlas
pages rather than allocating a layer for every frame. The >256-image GPU
regression passes. Quick tier passed all ten changed library crates, plus
binary-only bri-fx-import tested separately (tools/check.py assumes --lib).
Focused checks found and corrected Image fixture initializers in net and
sim integration tests; validation continues.

Private conversion checks exposed reversed original random bounds and a
scoped impact-sound binding error; corrected in private tooling, rather than
weakening engine validation. AR and Warthog effects merge with stock and
upload all 277 recovered images. No private fixture is committed here.

The original HUD atlases exposed an unrelated generic limit: kind Texture
allowed only 128 KiB encoded and 1024-pixel edges. Wider atlases now retain
original pixels with a 4M-pixel image cap and 64 MiB decoded package cap.

A bounded offscreen probe with the repository-pinned DXC compiled full scene
pipelines and drew on the RTX 4070 SUPER in 6402 ms. The old FXC log recorded
98056 ms for scene pipelines. This is diagnostic evidence, not a measured
end-to-end gameplay launch. Release packaging already ships the pinned DLL
and licenses; original installations and the running game are untouched.

Maxwell clarified the governing workflow: Halo exposes engine/modding gaps;
fix them as general capabilities, then use private Add-On integration and
human playtesting to discover the next gaps. Engine fixes precede polishing.


The private full-audio sweep reproduced the old silent-sample report: a PCM
WAV with a zero 20-byte fmt extension fails Symphonia. Fixed the common decoder
entry point, including streaming/restart, with in-memory header normalization.
PCM samples and unrelated RIFF chunks remain unchanged; nonzero extensions,
non-PCM codecs, malformed sizes and truncations retain decoder handling.
Synthetic preload/stream/restart and malformed-header regressions pass (2).
The private audio sweep is being repeated against that fix.

The private release checklist was recovered read-only from
origin/notes-backup:notes/v0.3.1/definition-of-done.md. It requires six community
maps and all six ModTer packages in the public bundle, and engine-only public
release assets. tools/release.py refreshes/rebuilds/uploads this bundle from
the main checkout after importer changes. Private Halo/Mario content stays
outside it. No checklist was marked complete without evidence.


Late focused verification (audio, package-runtime and client, depth 0 after
shared dependency coverage in the broad run) passed all 11 steps in 302 s:
clippy with warnings denied, audio/package tests, client units, weapon effects
and actor effects. The private decoder sweep passes all 182 original converted
files. The broad changed-crate/dependent run is still in progress.

The CI software-GPU download now retries transient HTTP failures three times before failing, while retaining the same pinned SHA256 checks. PowerShell parse validation passed. This addresses the observed 502 that forced a whole Windows shard rerun.


The broad focused run passed all 53 steps in 1647 s, including changed-crate
clippy, dependent integration suites, client units and targeted actor/weapon
and multiplicative-surface rendering. Together with the 11 late focused steps,
quick tier, six private checks and PowerShell parse validation, this is the
local pre-commit evidence. Full integrated gate, Windows PR checks and public
publication remain pending.

All six ModTer and six community-map source entries have local candidate
copies and pinned hashes. The main content root's remembered search now also
includes E:\Downloads while retaining its previous search folder; the release
refresh will verify/reimport them using the candidate importer.

Release refresh stopped on stale nested research-importer lockfiles: bri-convert now depends on the collision conversion stack. Refreshed both standalone importer locks offline, then verified cargo metadata --locked --offline succeeds for each. Runtime sources and the root lockfile are unchanged by this packaging repair.

## Release bundle follow-up: reference-only conversion failures

The first release preparation regenerated public base content and passed its
headless check, but the community bundle stopped on Arctic Challenge. Diagnostic
reproduction identified two unused reference assets (`1x1x5spike.blb`, an old BLB
fragment, and editor `octahedron.dts`, DTS v18), rather than map-owned failures.
Map import now reports base conversion errors as warnings and lets map_bundle
validate every referenced converted output. Map/borrowed Add-On errors and scan
errors remain fatal, with source-path diagnostics retained after scratch cleanup.
The synthetic map regression now includes a corrupt unused reference shape;
a separate broken-source regression keeps map-owned corruption fatal.

Initial `cargo fmt --all` exceeded Windows command length; formatting is per
crate through the tier runner, as documented. Full release preparation stopped
before cross-platform release builds, integrated gate, main push or tagging.
There is still no published v0.3.1 at this point.

Verification: `python tools/check.py focused --crates bri-addon-import --depth 0`
passed (3 steps, 171 seconds) after the source-path regression was made
case-insensitive. Arctic Challenge's real pinned source imported successfully:
1 map, 2 direct native assets, 7 original textures, zero unresolved textures.
