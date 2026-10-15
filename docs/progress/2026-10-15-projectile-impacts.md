# 2026-10-15 Projectile impacts by what they hit

A projectile can now name the explosion effect it makes by what it lands on:
`"impacts": { "player", "vehicle", "brick", "map" }`, each an explosion
effect as `explosion.effect` names one. A ray lands it as a flying round
does; a surface it names nothing for, and a burst in the air, keep
`explosion.effect`. Only the effect (and its explosion's sound) changes: the
blast's damage and push stay `explosion`'s. No protocol change: the host
already sends the effect it picked as an ordinary effect event.

Why: bullets should throw metal sparks off a vehicle and dust off the
ground (Max, for the Halo Add-Ons). The kinds are what the weapons runtime
already knows a hit to be (`TargetId`); surface kinds on the ground
(dirt, stone, metal floors) and per-surface decals can extend `impacts`
later without changing these four.

Evidence: `cargo test -p bri-weapons --test impacts` (3 tests: a ray and a
flying round each pick the vehicle, map or brick effect and fall back to
`explosion.effect`; an unknown surface key is refused).
`python tools/check.py focused` before the commit.
