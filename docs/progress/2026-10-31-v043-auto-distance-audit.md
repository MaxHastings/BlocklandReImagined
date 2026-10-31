# Issue 417: automatic draw distance audit

Issue 417 asks for an optional automatic draw distance that reacts to FPS and
focus; indoor detection is a suggested enhancement. The existing graphics
setting is a fixed 110–1000 unit `visibleDistanceMax` cap. It scales enabled
fog in `app/render.rs` and affects terrain, water and selected visibility
consumers, but the primary camera projection is built earlier with the fixed
4000-unit `FAR_PLANE`. Scene frustum culling uses that projection. Therefore
shrinking fog alone is not evidence that distant scene geometry was culled or
that frame time improved. Fog-disabled maps also do not get a distance fade
from the cap in that branch. No automatic controller or preference was found.

The platform already paces background frames at 50 ms and calls
`App::focus_changed`. Frame timing is presently requested only while the
performance overlay is visible, and includes presentation/acquisition wait and
intentional frame pacing. An adaptive controller using raw elapsed time would
mistake a selected frame limit, vsync, background throttling or a transient load
for rendering overload. The GPU timer is optional and asynchronous. No indoor
heuristic should be introduced without evidence that it improves the existing
zone/frustum culling and does not cut sightlines through windows or doors.

Maxwell questioned whether automatic distance is desirable given proper culling.
Prioritize measurements of existing frustum, zone, depth and occlusion behavior
and fix demonstrated unnecessary work first. Automatic distance is a possible
technique to evaluate, not an agreed feature: it can cause visible popping and
will not address unrelated bottlenecks. If evidence later supports it, any
design must preserve map fog, secondary views and the manual maximum, account
for focus and intentional frame caps, and demonstrate actual culling and frame
cost rather than fog alone. Do not assume an indoor heuristic is needed.
