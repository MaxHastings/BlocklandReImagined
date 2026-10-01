`Checkpoint::brick_inputs` and `Checkpoint::brick_outputs` become
`Checkpoint::brick_events` (`bri_events::Extension`): the running Add-Ons'
wrench event inputs, outputs and now targets (Slayer's `Team(Client)` and
`Team(Brick)`). Event rows gain `Target::Derived(name)`, a row aimed at one.
