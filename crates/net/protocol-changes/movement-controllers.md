`PlayerState::controller`: a movement controller's 16 bytes of memory and its
animation intent, in every own pose. `Checkpoint::archetypes` entries carry
`controller` (the controller's id and WebAssembly module), which clients
compile to predict with.
