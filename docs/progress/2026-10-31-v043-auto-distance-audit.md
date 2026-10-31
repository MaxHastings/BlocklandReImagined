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

Next implementation must connect an explicit, persisted opt-in to an effective
per-view visibility limit that actually reaches scene culling and the other
distance consumers; preserve map fog semantics, reflection/secondary cameras,
shadow fitting and the manual maximum. Use bounded, smoothed load evidence
with hysteresis and recovery, accounting for focus and intentional frame caps;
validate geometry submission and frame cost on representative scenes, plus
focus, loading spikes, capped FPS, toggling and a fog-disabled control. The
issue's suggested interior heuristic remains an evaluation, not an assumed
requirement. This audit establishes the gap and design constraints, not a fix.
