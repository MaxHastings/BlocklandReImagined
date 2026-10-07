# 2026-10-06 Far portals show their side's shadows

Max: with two portals far apart, looking through one showed no brick
shadows on the far side until he walked through.

## Cause

Every view of the world (the player's, each live mirror or window plane,
the environment probe's faces) read one set of sun cascades, fitted to the
player's view. A receiver picks its cascade by its distance from the
player's eye; one outside every cascade is drawn unshadowed. A mirror's
view stays near the player, so it was covered. A window onto a place 100+
units away was not: everything past it was lit, until the eye went through
and the cascades were fitted there.

The server had the same blind spot for other players: their poses go out
less often the further they are from each viewer (down to 5 Hz), measured
straight across, so a player seen through a nearby portal moved at the far
rate.

## Change

- `shadow.rs`: the receiver uniform has a region per view that can fit its
  own cascades (the player's, then each live plane, `FITTED_VIEWS`). Every
  region starts as the player's each frame; `ShadowMaps::fit_view` fits a
  plane's own cascades (same settings, its own caster matrices) and writes
  its region. `ShadowMaps::reach` says whether the player's cascades hold a
  point, the same test receivers make.
- `scene.rs`: each view's camera group binds its region.
  `SceneRenderer::render_view_shadows(view, ...)` draws view 0's shadows as
  before (kept brick layers, lamps, map light cubes) or a fitted plane's sun
  cascades only, directly into the same layers. No extra shadow texture.
- `reflection.rs`: `prepare` gives a plane its own cascades when the
  player's do not reach its eye, or its parent view has its own. They are
  fitted to the player's whole frustum moved by the plane (the plane's own
  projection is oblique and cropped, which has no usable far plane).
  `render_views` takes a `shadows` callback and draws the planes with their
  own shadows first (each right after its shadows), then the player's
  shadows, then the other planes, so every pass reads the right layers and
  still follows what it shows. Planes are capped at `MAX_PLANES`, which the
  probe's view numbers already assumed.
- Client: casters are gathered once; with live planes the player's shadows
  are drawn by `render_views`, otherwise directly as before.
- `net/stream.rs`: pose distances use `Passages::shortest`, so a player seen
  through a portal is as near as they look.

Costs: one extra sun-cascade draw per live far window (no kept brick
layers there), nothing when no window looks far. With pass timing on,
those draws count in the first stretch ("sun shadows").

Not changed: lamp shadows (Unified lighting) are still picked near the
player, so a far side's lamps light it without live brick shadows.

## Evidence

- New `crates/render/tests/mirrors.rs`
  `a_window_onto_a_far_place_shows_the_shadows_standing_there_would`: a
  window 1000 units from its partner; the floor's block shadow seen through
  it matches standing there. Fails with the plane fit disabled (639 of 802
  shaded pixels: only the block itself), passes with it. All mirror tests
  pass on lavapipe.
- New `stream.rs` `a_player_seen_through_a_portal_is_as_near_as_they_look`.
- Cloud run: `cargo clippy -p bri-render -p bri-net -p bri-fx-runtime
  -p bri-client --all-targets -- -D warnings` clean; `cargo test -p
  bri-render -p bri-fx-runtime` and `cargo test -p bri-net --lib` pass.

## For Max to check on the PC

1. A map with two portals far apart (100+ units), Shadows on, Brick Shadows
   on, a build with clear shadows by the far portal.
2. Look through the near portal: the far build's shadows show, the same as
   after walking through.
3. Walk through and back: no pop of shadows appearing or vanishing.
4. A mirror nearby still shows shadows as before, and frame rate looking at
   a mirror is unchanged.
5. With a friend or bot running on the far side, they move smoothly through
   the portal (not choppy).
