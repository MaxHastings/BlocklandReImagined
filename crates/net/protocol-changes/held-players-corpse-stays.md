# Held players and bodies that stay

`Vitals` gains a trailing `held` (a rule holds the player out of the world: no body is simulated or drawn), left out when false. `Look` in the archetype table gains `corpse_stays` (the body lies until its player spawns again), left out when false. Positional wire arrays make the skipped trailing fields safe for older readers of the same shape.
