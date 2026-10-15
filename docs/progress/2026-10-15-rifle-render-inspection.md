# 2026-10-15 Rifle render inspection

The private assault-rifle inspection exposed shared presentation defects. This
entry records a candidate, not fidelity or playtest acceptance. Halo assets and
the local capture program remain outside public source.

- Imported RGBA textures can explicitly opt out of legacy node-paint overlays.
  The original display texture was incorrectly painted white through transparent
  pixels. Legacy materials retain their existing behavior by default.
- The rest-pose shortcut now considers skin-bone animation as well as object
  nodes, and retains live material values when choosing a rest pose. Previously
  image geometry could remain still while its sockets animated, and live digits
  could revert to zero.
- Images can select authored first-person effect definitions. This preserves
  the distinction in source effects without making the engine identify a game.
- Magazine reloads can request a separate one-shot image animation alongside
  the body action. Static support-hand IK yields while a body action is active.
- Sprite casing emission uses the same socket-axis conversion as image effects.

Evidence: the changed-crate quick tier completed 12 steps in 261 seconds. Two
synthetic client regressions cover skin animation ownership and RGBA semantics;
a weapons regression covers distinct body/image reload routing. These establish
mechanics, not appearance. A private ordinary-runtime trace records ammo changing
from 59/240 to 60/239 and separate reload animation events.

Twenty bounded offscreen captures use the actual item/avatar/effect renderers,
original converted private assets, a diagnostic floor and an occluding wall.
The latest captures show blue 60/08 digits and original reload poses, with recovered
rifle material layers. They are not captures of Blood Gulch or matching original
game conditions. No interactive session or video was recorded. Evidence is in
the main checkout's ignored artifacts/reference-first-rifle-material-review.

Open: flash brightness/composition and casing trajectories still require review;
the capture's large time step is not a timing certification. Procedural sway is
estimated, not recovered CE behavior. Audio events are traced, not listened to.
HUD, slopes and Warthog inspection remain open. Warthog review must include turret
axes, muzzle/ejection sockets, effects, all seats, materials, windshield and lights;
Maxwell owns interactive driving/aiming feel and final acceptance. No release or
acceptance checkbox is closed by this evidence. The new release executable is
building; content installation and headless verification follow completion.
