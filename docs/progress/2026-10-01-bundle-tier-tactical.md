# Bundle: Tier+Tactical originals (2026-10-01)

Added Kai's Tier+Tactical packs to `packages/default-addons.json` as bundled
originals, all off by default and credited to Kai: Tier 1, 1A, 2, 2A,
Explosive 1 and 2, Medical, Melee Extended I and II, and the 12 weapon skin
packs (21 entries), plus the Impact Rifle. Each sha256 is the pin from the Tier lane's
`crates/addon-import/ports/ports.json` at 3809ecea; the Impact Rifle pin (65a13321…) is from Kai's copy, verified by the Gate (3/3, datablocks 14/14).

Held back until ported: Frogs, Frogs WWII, ShortRifleKai and Event_AddAmmoTT.

This lands with or right after the Tier lane, so the bundle build applies the
Tier ports to the real copies.

Checks: `cargo test -p bri-package` (40 passed), `cargo test -p
bri-addon-import --test bundle` (4 passed).
