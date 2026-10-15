# 2026-10-15 Warthog source, interior and contact review

Root worktree: `playtest-v031`, branch `codex/v031-remaining-capabilities`.
Private conversion work remains in the separate private Add-On worktree.
No interactive game was launched and no original installation was modified.

## Rendering and recovered bindings

The generic shape alpha-cutoff capability and original material classification
correct the observed disappearing attached gun. Matched high rear diagnostic
captures differ only in the chain-gun material's old alpha blend versus new
opaque mask. The corrected material retains its visible surface/depth against
the hull. This is defect evidence, not original-game fidelity acceptance.
The original fixed alpha threshold remains unknown; native mask uses 0.5.

Mounted vehicle lights now retain full authored node frames and accept generic
switch/driver/always activation and native effect references. Light coronas may
have authored angle falloff and depth testing, including radius-zero coronas
without extra point illumination. Flare textures load from their own package.
Occupied/empty and day/night renders show the private vehicle's four original
corona markers. Original projected gel/cookie illumination, light power ramps,
taillight activation and exact source lens occlusion remain open.

A new close interior diagnostic view exposed pale gauge rectangles and a static
sensor sweep: native static material import is insufficient for the original
external-value meter shaders and animated additive sensor material. No painted
replacement or claim that the dashboard is complete was made. The camera is an
inspection position, not a verified driver eye or original-matched environment.

## Contact and handling

Maxwell reported dirt from an airborne vehicle thrown with a gravity tool.
Root cause: private rear-hub dust was bound as an ordinary speed-only trail,
so total falling speed triggered it. Added optional Trail.contact_wheel, validated
against the definition. Such trails require their own authoritative wheel contact
and use chassis-tangential speed. Ordinary airborne trails retain total speed.
The private generator and current package bind their rear dust to the rear wheels.
Occupancy is deliberately irrelevant: an empty rolling vehicle can disturb dirt.
Original dust art and contact-normal/surface-specific tire presentation remain open.

Also corrected a spring coordinate mismatch: turret response was measured in
chassis axes but applied after the authored mount and gun yaw/pitch. It now
measures acceleration in the same full attachment frame. This does not implement
independent yaw inertia or vertical node travel; current pitch/roll spring tuning
is an adaptation, not recovered original physics.

A/D currently overrides the mouse target while held, then resumes the camera
target on release. No simultaneous numeric cancellation occurs in that branch;
abrupt release handover, turning radius at speed, suspension and subjective feel
remain under review. Original vehicle turn tags alone do not establish their
runtime unit/control law. Do not retune arbitrary numbers and label them original.

Maxwell supplied original passenger third/first-person references. Source has a
distinct passenger marker/camera and the converted Chief has passenger-specific
rifle/pistol/missile idle clips. The prior forced driver-height override was too
simplistic. Correct marker/pose/eye pairing remains open; no blind reversal made.

## Evidence and checks

- Headless private capture: 14 views completed, including close dashboard, three
  body types, occupied/empty, lamp night view and minigun overlap. No GPU validation
  or asset/checksum fault in that run. Private art/captures remain outside public Git.
- New ground-trail regression passes: falling without contact, wrong-wheel contact,
  vertical landing speed, empty rolling motion and preserved airborne contrails.
- Existing actor-effects test constructor initially failed to compile after earlier
  Image fields were added; updated missing defaults, then the regression passed.
- Earlier mounted-corona directional/depth and own-art/lifecycle regressions pass.
- `python tools/check.py quick --crates bri-client,bri-vehicles,bri-content,bri-fx-runtime,bri-weapons,bri-convert,bri-addon-import,bri-weapons-import`:
  all 16 format/unit steps passed in 199 seconds. This is regression evidence only.
- `cargo fmt --all` exceeded Windows command length; per-package formatting passed.

No new release executable/content installed, commit/push, release tag or publication
in this increment. Latest release rebuild, focused/integrated gate, exact packaged
candidate verification and Maxwell's interactive acceptance remain open.
Private `halo/COVERAGE.md` is the canonical detailed source/runtime/acceptance ledger.
