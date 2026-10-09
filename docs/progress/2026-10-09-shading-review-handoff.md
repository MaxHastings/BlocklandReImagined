# 2026-10-09 Soft Shading/AO review handoff

Max asked to wrap up and push the current work for Claude's review for v0.2.8.
Branch `codex/sky-shading-preview`, draft PR #35. Nothing is merged to main.

Enhanced Sky is removed, including the Environment toggle, client Sky option,
script setting and procedural renderer. Maps always keep their authored skies.
Soft Shading and Ambient Occlusion keep their two short Options labels and
operate only in Unified/Dynamic. No additional graphical controls are added.

Indoor tint now uses geometry-derived upper-hemisphere visibility, independent
of sun direction, map names and Shadow Quality. Persistent native map/build
geometry is cached per view; moving characters, vehicles, items, debris and
Add-On models overlay the depth cache without contaminating it. Masked surfaces
retain cut-outs; blended surfaces do not block this opaque visibility field.
This is a local approximation of direct skylight visibility, not multi-bounce
global illumination. Nine 512-square depth projections cover a camera-centred
96-unit region, fading to authored ambient outside it. Moving receiver normals
and positions sample the same field; mirror/portal cameras have their own field.

Verification on Max's PC, entirely offscreen:
- `cargo check -p bri-client -p bri-render --tests --locked`: passed.
- `python tools/check_format.py`: all workspace packages passed. This wrapper
  avoids Windows command-length error 206 without omitting packages.
- `cargo clippy -p bri-client -p bri-render -p bri-ui --all-targets --locked -- -D warnings`: passed.
- `cargo test -p bri-render --test lighting_environment --test shader_validation`:
  9 lighting tests and 3 shader-validation tests passed. New semantic regression
  covers an open floor, static roof add/remove and moving roof movement/removal
  on the same cached renderer in Unified and Dynamic. Enclosed floor ambient is
  preserved, open floor is tinted, and no channel brightens.
- `cargo test -p bri-render --test mirrors`: final-source rerun passed all 12
  ordinary tests in 56.47 seconds, including AO/mirror/portal compositing. One
  preexisting hardware benchmark is ignored by the default test invocation.
- No `crates/client/src/items.rs` or Gravity Gun package delta against main.
- Source search confirms Enhanced Sky controls/settings/render code are gone.

Remaining acceptance, explicitly not signed off: Bedroom and Kitchen native
renders of this exposure change, window exposure quality, dense-build exposure
refresh cost and AO GPU timing at 1080p/1440p with MSAA on/off; strict 4x MSAA
Bedroom exterior parity (previous preview differed at 12/17/35 pixels); final
full local gate and Windows CI. Prior r2 mirror/portal, Glow, glass, character,
vehicle, particle and native GUI regressions passed. No new test exclusion or
unproven known-main failure exception was introduced.

The downloaded r2 binary still contains Enhanced Sky and the old global tint;
it does not represent this source. A new artifact-only preview will be built
from runtime commit `cf45c617` (`publish=false`), version
`preview-2026-10-09-shading-r3`:
https://github.com/MaxHastings/BlocklandReImagined/actions/runs/37987081663
It is building, not yet a downloadable verified zip. PR remains draft for review and
verification; this entry is not a merge approval. Max performs all interactive
playtesting. Original content, screenshots and local logs remain uncommitted.
