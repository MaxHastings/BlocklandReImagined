# 2026-10-18 Projectile contrails

A generic weapon presentation seam from the Halo lane (`codex/halo-contrail`),
for the original plasma bolts' contrails. Inert unless a projectile sets it.

Before: a projectile left nothing behind it except an emitter's particles
(`trail`), which cannot draw a continuous ribbon.

After: `projectile.contrail` (`texture`, `rate`, `keys`, `additive`) draws a
camera-facing ribbon of textured quads through the points the projectile has
passed. A point is made `rate` times a second; a quad runs from each point to
the next, and its width and colour follow the keys by the point's age (linear
between them), gone at the last key's age. The ribbon runs out after the
projectile does.

How: `bri_client::contrails::Contrails` keeps each shown projectile's points
(presentation only, nothing sent) and rewrites one dynamic ribbon mesh per
projectile each frame (a quad from each point to the next, turned to the eye,
vertex width, colour and texture coordinate by the point's age; the texture runs
along the ribbon `repeats` times over the last key's age). `WeaponEffects`
feeds it the projectiles in view; the app advances, uploads and draws it with the
world's scenes. Tests: the ribbon follows a projectile and runs out, it thins,
fades, squares to the flight and its texture runs continuously along it, schema
limits.
