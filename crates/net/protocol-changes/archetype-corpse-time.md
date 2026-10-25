`Look::corpse_seconds` and `Look::corpse_fade` in the archetype table the host
sends (`Checkpoint::archetypes`, `Delta::archetypes`): how long a dead body
lies and how much of that it fades out over. Clients now time a corpse by
`Vitals::died_tick` and the host's tick, not by when they first saw it.
