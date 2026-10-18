# 2026-10-18 Projectile homing

A small generic weapon seam from the Halo lane (`codex/halo-covenant`), for a
guided plasma bolt. Inert unless a projectile sets it.

Before: every projectile flew straight at its launch velocity.

After: `projectile.homing` (`turn_rate` radians a second, `range`, `angle`)
makes a projectile pick the nearest player or vehicle (not its shooter, nothing
the collision adapter refuses) within range and inside a cone around its
heading as it is made, and turn toward it each tick at up to the turn rate,
keeping its speed. It flies straight with no target or once its target is gone.

How: the host's `projectile_step` calls `steer` before the sweep; the target
is kept host-side per projectile. Clients coast straight between updates and
the host's stream already resends a projectile that has left its coasted
path, so no wire change. Tests: `crates/weapons/tests/homing.rs` (curves onto
a target a straight shot misses, speed kept, outside the cone not followed,
schema limits).
