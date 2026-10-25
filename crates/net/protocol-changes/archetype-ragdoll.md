# Archetype ragdoll

`Look` in the archetype table the host sends (`Checkpoint::archetypes`, `Delta::archetypes`) gains `ragdoll` (`bri_content::body::Ragdoll`: bones with shapes, mass, damping and joints; friction, bounce, speed limits, freeze), left out when none. Absent leaves a body playing its death clip as before.

`Ragdoll` also carries `kick`, `carry`, `falling`, `inertia`, `spin_scale`, `prone_scale` and `random_spin` (the killing hit's effect on the body), and `bri_content::body::DeathKick` is new. The weapons pack's `DamageType` gains a trailing `death_kick`, left out when none.

`DeathPose` (in `Vitals::death`, so in checkpoints and deltas) gains a trailing `hit`, left out when the body has no ragdoll: `DeathHit{direction, point, kick, prone_shot}`. Positional wire arrays make the skipped trailing field safe for older readers of the same shape.
