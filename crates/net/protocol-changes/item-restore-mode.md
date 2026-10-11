`ItemSpawn::restore` (`timer` or `reset`) travels with every brick's item
spawn: in the world a joining client receives and in brick deltas, in
`ToolAction::SetWrench::properties.item_spawn`, and as
`WrenchFill::item_restore`. `reset` means the taken item stays gone until a
mini-game reset or a `restoreItem` event; a client draws such an item as a
ghost by its `available_at`, as before.
