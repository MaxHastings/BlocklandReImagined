An original brick may carry transient damage_attachment pointing to the one
generated piece that carries its existing light/emitter/sound. Normal public
brick deltas and late-join checkpoints preserve the field through whole-brick
packing (a sparse entry-indexed attachment column for otherwise plain bricks). It is valid only on hidden sources, is selected by authoritative
partition geometry, and is cleared on restoration/canonical saving. It is a
visual relationship, not a gameplay transform or asset/settings copy.

Alpha wire change: Brick adds optional damage_attachment. Canonical content and
saved builds do not acquire this transient field; no save migration is added.
