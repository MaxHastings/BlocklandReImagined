# 2026-10-07 Vehicle wreck pieces drew white

Max, playing v0.2.5: after a vehicle explodes, white copies of parts of the
model are left behind; maybe only tanks.

## Cause

The client's vehicle loader (`crates/client/src/vehicles.rs`) looked for a
DTS material's texture as `<name>.png`. Some stock models name their
materials with the extension already (`black.png`, `Gray.png`), so the
lookup became `black.png.png`, missed, and fell back to the blank paint
surface. Blank paint is laid over the instance tint, and explosion debris
is drawn with a white tint, so those pieces showed white.

Checked against the v0.2.5 release's `vehicles-pack-012` (every model
material, old rule against new):

| Model | Materials that missed | Seen as |
|---|---|---|
| `Vehicle_Tank/tankDebris.dts` (hull thrown by the final explosion) | `black.png` | white hull, about 3 s |
| `Vehicle_Tank/tank_turretDebris.dts` (turret thrown when it blows) | `black.png` | white turret, about 3 s |
| `Vehicle_Pirate_Cannon/CannonDebris.dts` | `Gray.png` | white barrel |
| `Vehicle_Tank/tank_turret.dts` (live) | `blank.png`, `black.png` | barrel in the paint colour, not black |
| `Vehicle_Pirate_Cannon/Cannon.dts`, `CannonBall.dts` | `Gray.png`, `blank.png`, `BBlack.png` | grey and black parts in paint/white |
| `Vehicle_Rowboat/rowboat.dts` | `blank.png`, `black75.png` | dark trim in paint colour |

The Jeep, Ball, Horse, Magic Carpet and Skis name theirs without the
extension and were already right. The debris itself is client-only and
expires (lifetimes 2 to 3.5 s in the datablocks); nothing was left forever.

## Fix

`material_texture` tries the name as written, then with `.png`, `.jpg`,
`.jpeg`, beside the model first and then anywhere in the pack, the same
order the Add-On importer already uses for item textures. Every stock
vehicle material now finds its texture (checked with the same script over
the release pack). Test: `vehicles::tests::a_material_named_with_its_extension_finds_its_texture`.

## Broken tank turret drawn twice

The turret explosion (`tankTurretExplosion`, fired when the turret is shot
off, or when the hull dies with its turret still on) throws
`tank_turretDebris.dts`, the same turret mesh. The client kept drawing the
attached turret on the hull anyway, so one flew off while another stayed.

Max did not remember what v20 does and asked for best judgement. Choice:
the debris is the turret, so once broken the hull no longer draws it.
`VehicleInfo::turret_broken` (new, reliable, sent on change; protocol file
`vehicle-turret-broken.md`) carries the host's turret state. The client
skips the attached turret when it is set, and draws the gunner on the hull's
fallback seat, where the host already moves them
(`effective_seat_pose`), instead of floating where the turret was.
Test: `vehicles::tests::a_broken_turret_leaves_the_hull_and_its_gunner_sits_on_the_hull`
(synthetic pack, and the content pack when present).

`docs/audits/vehicle-destruction.md` row 1 said the wreck's turret is
painted black. That stays true for the paint rule, but a destroyed tank's
turret has always been blown off first (the hull's death fires the turret
explosion when the turret is still on), so the wreck now shows no turret.
