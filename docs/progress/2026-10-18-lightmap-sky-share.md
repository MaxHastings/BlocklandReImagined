# Lightmaps follow the sky at night (2026-10-18)

Max's playtest: Blood Gulch at night looked wrong; the Night look and the day
cycle at midnight both showed warm brown terrain under a dark blue sky.

Cause: a baked lightmap on an imported shape replaces the live sun and ambient
(`physical_light`), so under any environment its daylight stayed. Offscreen
renders (noon/midnight/dusk, as shipped, without lightmaps, with the old
exponential fog): shipped midnight yard mean RGB 36.9/26.6/19.3 against
6.8/6.8/11.6 without lightmaps; noon is the same either way.

Fix (generic): `SceneData::lightmap_sky_share`, read from the sun node's
`lightmap_sky_share` property, stored in `Camera::baked_ambient.w`. When the
live environment differs from the baked one, that share of the lightmap light
is multiplied by (live ambient + sun) / (baked ambient + sun) per channel,
never above 1; interior lightmaps with a decomposition scale their static
light the same way and drop the plain ambient delta by that share. A map that
does not say keeps 0 and behaves as before. Blood Gulch sets 1.0 (flagged:
inferred; its lightmaps are radiosity from sun and sky, the four beacon lamps
are live lights).

After: shipped midnight yard 7.2/6.9/10.5, red base 3.2/3.2/4.0; noon
unchanged 85.2/74.4/77.0. Tests: `crates/render/tests/lightmap_sky_share.rs`;
the Add-Ons repo's `halo/tests/bloodgulch_night.rs` renders all three times.
