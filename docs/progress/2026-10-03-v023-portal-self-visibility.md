# First-person portal self visibility

Local v0.2.3 candidate, not released or interactively accepted.

Max reported a half hat/body appearing while halfway through an opening in
first person. The main pass excludes the local body, but reflection passes
previously drew every body, including both clipped copies of the local body.
A portal's virtual camera can occupy that body's own eye during the crossing.

The existing reflection plan now permits per-view model lists. Only the local
first-person body and its world-space held-image copies are omitted when a
virtual eye coincides with that body's original or actually carried camera
anchor. Third-person bodies, other actors, distant views of oneself and shadow
casters retain their existing paths. The existing camera-tilt calculation was
extracted for reuse; this increment does not change camera travel.

Semantics: the uncarried body eye is the anchor, not the main eye which may have
already crossed ahead of the body's middle. The 2 cm numerical tolerance covers
reflection clip clearance, not a general proximity fade. A review caught an
incorrect inverse-eye candidate: only the original and current Straddle carry
are actual rendered copies. That third location was removed and a translated
quarter-turn counterexample added. Current straddle is resolved each frame,
including package bodies hidden from the ordinary avatar draw list.

Evidence:

- `/tmp/bri-v023-portal-body-policy-suite.log`: six portal-view checks pass,
  including actual reflection plans around translated/rotated crossings and a
  nonexistent inverse-copy counterexample.
- `/tmp/bri-v023-portal-model-views.log`: all seven offscreen mirror checks pass,
  including 1x/4x MSAA model omission and retained portal/mirror rendering.
- Initial test assumed a camera already through and looking forward would
  still render a portal behind it. That fixture expectation failed; the
  crossing-side case now deliberately looks back at the still-straddling body.
  Receipts `/tmp/bri-v023-portal-eye-policy.log` and `...-2.log` are retained.

Mounted Horse passage rejection and Jeep camera discontinuity are separate
reports under investigation. This visibility change does not claim to fix them.
Actual in-game first-person crossing remains Maxwell's acceptance check.
