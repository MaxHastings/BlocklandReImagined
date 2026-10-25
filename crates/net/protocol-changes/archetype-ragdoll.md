# Archetype ragdoll

`Look` in the archetype table the host sends (`Checkpoint::archetypes`, `Delta::archetypes`) gains `ragdoll` (`bri_content::body::Ragdoll`: bones with shapes, mass, damping and joints; friction, bounce, speed limits, freeze), left out when none. Absent leaves a body playing its death clip as before.
