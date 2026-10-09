# 2026-10-09 Practical sky and shading preview handoff

Max asked to stop expanding verification and deliver a practical preview for
his interactive playtest. This preview is not a main landing or completion of
the full alpha contract. Branch `codex/sky-shading-preview`, draft PR #35;
packaged runtime source is 9ca5ffd42. Nothing is merged to main.

Changes: keep host Enhanced Sky opt-in and player Original override; preserve
Classic/off rendering through the unchanged main scene shader; scale generated
sky to authored map darkness, soften dusk saturation and enlarge the sun disc;
keep Soft Shading RGB from intentionally brightening floors; apply AO between
opaque and blended passes, fade it in fog, and use an equal-depth coverage mask
to exclude visible Glow/unlit geometry without excluding hidden geometry.
Gravity Gun merge c4d7a2e is reverted. The diff against origin/main 3c8ed1f
contains no gravity-gun package or client items.rs changes. No original game
content is committed. Enhanced Sky currently has no procedural daytime clouds;
separate cloud layers are retained, but clouds painted into a replaced skybox
are absent from the generated sky.

Evidence on Max's Windows PC (offscreen only):
- `cargo clippy -p bri-client -p bri-render -p bri-ui --all-targets --locked -- -D warnings`: passed on 9ca5ffd42.
- Renderer library: 58 tests passed; lighting_environment: 12 passed;
  shader_validation: 3 passed, including frozen original and modern shaders.
- Atmosphere: 8 passed; Python tool tests: 38 passed.
- Main reference renderer is origin/main 3c8ed1f. Paired Bedroom floor, window
  and outside renders match exactly in Classic, Unified and Dynamic with
  MSAA off. With 4x MSAA, floor/window views match; outside differs at 12
  Classic, 17 Dynamic and 35 Unified pixels of 1920x1080. Differences are
  unresolved and pixel-perfect parity is not signed off. The reference shader
  copy hashes identically to main (0ef67daf047062f4d46bd94e6effc15d511f7b26).
- Earlier Slate/Skylands paired checks passed, but their ground is collision
  geometry, so these views are not evidence for visible-floor brightness.
- Offscreen Bedroom window render verified the corrected camera and dusk
  adjustment. Glow exclusion (MSAA on/off), hidden Glow, complete fog, floor
  brightness, crease darkening and split opaque/blended pass have render tests.
- Native mirrors passed on both main and preview (11 passed, one preexisting
  ignored benchmark). Overlapping work makes earlier timings unsuitable for
  claiming a performance regression or a measured AO budget.

Outstanding: full map matrix, strict 4x MSAA exterior parity, full-map channel
brightness checks, fair GPU timings and software-GPU CI stability. Max will
judge the look and frame rate in this preview; none is falsely marked accepted.
`python tools/gate.py` without push is running on the final runtime source;
its previous attempts were interrupted for code fixes, so no full gate pass is
claimed yet. No tests were disabled or added to known failures without evidence.

Release workflow dispatched on this branch with
`version=preview-2026-10-09-sky-shading`, `publish=false`:
https://github.com/MaxHastings/BlocklandReImagined/actions/runs/37974800673
It checks and packages Windows, then builds Mac/Linux from the same source and
Windows content artifact. No GitHub release or version tag is created.

Player checklist: host Enhanced Sky on/off; Original player override;
Bedroom Dark brightness; sunset/twilight colours and visible sun; Unified and
Dynamic Soft Shading/AO and floor brightness; water/glass/see-through/Glow;
fog crease fade; all-off and Classic against v0.2.7.2; big-build frame rate.
Future main landing still requires Max to approve each separate feature PR by
name, then `python tools/gate.py --push` on this PC.
