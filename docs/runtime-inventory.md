# Native tool inventory integration

The host owns one five-slot inventory for core tools and native weapon items.
Stock spawn order is Hammer, Wrench, Printer, empty, empty, from recovered
`allGameScripts-Vanilla.cs:9697–9699`. No Gun is granted implicitly. Core tool
actions remain the building authority's responsibility; these four item IDs
share inventory/drop rules without pretending to have weapon image graphs.
The original four core definitions all declare `canDrop = 1` at source lines
10423, 10763, 11913 and 23950.

`bri-weapons` accepts the four explicit core IDs alongside validated pack items.
Duplicate grants and full inventories reject before mutation. Core/weapon drops
and pickups use the same slots and persistence. Weapon saves now write schema2
and read schema1/2; restore also rejects duplicate inventory items. This does not
add a legacy reader to runtime. All native 17 weapon entries retain pack validation.

`Session` creates stock inventories on join/re-spawn via authenticated resume;
disconnect removes the actor. `set_weapon_pack` is setup-only. `give_item` is an
internal host operation, not a remote command. `EquipTool` resolves owner from
the connection and retains the existing replay/rate checks and image-state
switching restrictions. Session snapshots are now schema4 with weapon views.

Protocol7 replicates inventory, selected slot and weapon views to peers and late joiners.
Replica validation checks bounds, unique IDs, selected occupied slots and exact
connected-owner membership before committing any part of the delta. Normal
client tool/brick/paint selection now sends equip/unequip commands. The actual
App/QUIC headless flow asserts that selecting Printer reaches server authority
and returns in its replicated inventory.

## Evidence

- Session tests: stock defaults, per-connection selection, stale/empty rejection,
  internal grants, no remote Give command, snapshot and reconnect.
- Weapon runtime: mixed core/weapon slots, drop and first-empty-slot reuse,
  source pickup cooldown, cross-actor pickup, save/restore and duplicate rejection.
- QUIC: two peers plus late join observe selection; invalid selection rejects
  without altering the other player's inventory.
- Malformed replica inventory cannot partially apply a simultaneous brick edit.
- Actual release App/QUIC/UI/offscreen integration and weather tests pass; no
  visible window, input automation or audio playback.

## Still required before alpha

This is an integration checkpoint, not completed weapon gameplay. Normal host,
join and dedicated startup now load/hash native weapon definitions/resources
(content identity8) and expose all21 catalog items. Native presentation pack
loading/hashing still needs connection. Dynamic item metadata,
authoritative HUD reconciliation/rejection handling, core tool-use equipment
gates, world item spawning/collision pickups/respawn, drop commands and presentation
remain to connect. The existing client tool mapping still assumes initial stock
slots. Weapon fixed-tick queries and projectile replication now work (see
runtime-weapons-host.md); minigame damage policy, remaining runtime event handling,
mounts/animations, projectiles and audio/FX must be wired into Session/client/net.
Dedicated world saves do not yet include weapon runtime state. The existing
runtime's isolated drop/weapon tests do not establish any of those host behaviors.
