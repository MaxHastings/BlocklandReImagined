# 2026-10-15 Authored particle tint interpolation

Source effect audit found HSV tint flags discarded by private conversion. Native
ParticleVariation now selects RGB (legacy default), short-hue HSV or long-hue HSV.
The runtime samples one tint at birth, retaining seeded emission/position/alpha
and lifetime behavior. HSV supports authored HDR values; achromatic endpoints
borrow the other hue. Lifetime color keys retain their existing interpretation.

Private conversion binds actual flags and records source/native choices. Rifle:
152 HSV/11 RGB definitions, including 40 unequal HSV endpoint pairs. Warthog:
69 HSV/87 RGB definitions, including 37 unequal HSV endpoint pairs. Generated
private textures are unchanged. This mainly affects impact/dust color ranges;
constant flash colors are not a solution to flash scale/composition problems.

Checks: quick content/effects 4 steps/15s; focused content/effects 6 steps/39s
(clippy, unit/integration and offscreen GPU checks). Added runtime check passes:
HSV remains saturated, shares RGB's seeded positions/alpha, and tint remains
constant over time. Client/converter cargo check passes in 30.04s. New runtime
check added after focused run; final all-target effects clippy also passes.

Fresh current rifle diagnostic captures show muzzle attachment, original live
counter art and wall occlusion. The flash remains visibly large/bright; the
diagnostic floor, lighting and camera are not a matched original-game scene.
No original visual acceptance or gameplay acceptance is claimed. Delayed events,
event skips, particle lighting, source blend laws and actual target shield/flesh
bindings remain open. Release/playtest delivery is still incomplete.
