`Notice::Blueprint`: a held copy now travels compact (`Blueprint::kinds`,
`Blueprint::prints` and `CopyBrick`s that index them) and holds at most
`MAX_GHOST_BRICKS` (10,000) bricks of the copy, spread through it, for the
ghost; the whole copy stays on the host.
