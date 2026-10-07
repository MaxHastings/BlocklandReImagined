# 2026-10-07 Portal shadows near the edge of the shadow distance

Max's report after v0.2.5: looking through a portal at a place some way off,
the far side shows no sun shadows (fence, lamps, buildings), while standing
there they are clearly drawn.

## Cause

The v0.2.5 change (`2026-10-06-portal-far-shadows.md`) gave a plane its own
sun cascades only when the player's cascades did not reach the plane's eye
(`ShadowMaps::reach`). That tested one point, and loosely: a point counted
as reached anywhere inside a cascade's whole light box, which runs 400 units
(`CASTER_REACH`) towards the sun. A partner standing ahead of the player,
inside the shadow distance, or sunward of the player's view passed, so the
window read the player's cascades. What the window looks at lies further on,
in the last tenth of the distance where the shadows fade out, or past it
where there are none.

## Change

Every live mirror and window plane fits sun cascades of its own (the
player's frustum moved by the plane). `reach` and `shadows_reach` are gone.
The player's are fitted to the player's frustum, and what a plane shows lies
elsewhere (anywhere through a window, behind the player in a mirror), so
no test of a single point can tell when the player's would do.

Cost: while a mirror or window is live, the sun's casters draw once more per
live plane (at most `ReflectionSettings::MAX_PLANES`), without the kept brick
layers the player's cascades use. Far portals already paid this since v0.2.5;
near portals and mirrors now do too. Mirrors also gain shadows behind the
player, which the player's forward-fitted cascades only partly held.

## Evidence

- New test `a_window_onto_a_place_at_the_edge_of_the_players_shadows_shows_them`
  (partner one shadow distance ahead): failed before the change (639 of 799
  shaded pixels through the window), passes after.
- `cargo test -p bri-render` all pass on lavapipe (software Vulkan) in the
  cloud; `cargo clippy -p bri-render -p bri-client --all-targets -D warnings`
  clean.
- Not checked: the real game on a GPU, and the frame-time cost with a big
  build in front of a mirror. Needs Max's look before merge.
