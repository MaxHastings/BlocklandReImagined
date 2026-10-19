# 2026-10-18 Linear fog

A small generic fog mode from the Halo lane (`codex/halo-haze`), for Blood
Gulch's haze. Inert unless an environment sets it.

Before: every fog was exponential, reaching 98 percent of its authored depth
at the density distance, so a capped haze was already most of the way to its
maximum by half that distance.

After: `Fog.linear` makes the fog grow in a straight line from `start` to its
`max` at the density distance (`density_end`, else `end`), with no thinning
above the eye. Halo's atmospheric fog density texture is a linear ramp, so a
Halo sky's haze now reads half as thick at half range, as in the original.

How: `Fog::amount_along` returns the line when `linear`, and the shared
`fog.wgsl` does the same through the fourth `haze` component (0 exponential, 1
linear), so every pass agrees. The sky's horizon haze and the visible-distance
cutoff are unchanged. Tests: the content fog (the line, level in every
direction, the uniform) and a GPU test that fogged geometry matches the CPU
twin. No wire change.
