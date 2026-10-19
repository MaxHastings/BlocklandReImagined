# 2026-10-18 Projectile lens flare

A generic weapon presentation seam from the Halo lane (`codex/halo-contrail`),
for the original plasma bolts' lens flares. Inert unless a projectile sets it.

Before: a projectile's light (`light_radius`, `light_color`) lit the world but
drew no sprite, so a glowing bolt was only as visible as its trail particles.

After: `projectile.flare` (the effects library's `Flare`: texture, colour, size
and distance range, blend) draws a sprite at the projectile, on the light it
carries. It needs a `light_radius`.

How: the generated projectile light takes the flare; its texture is found as a
particle's Add-On texture is, so a pack names it by presentation key. The effects
runtime already draws a light's flare, sized by the camera's distance between
`near_distance` and `far_distance`, so nothing new is drawn. Tests:
`crates/weapons/tests/projectile_flare.rs` (loads, limits).
