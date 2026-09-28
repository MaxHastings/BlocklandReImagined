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

## Current state

Normal host, join and dedicated startup load and hash native weapon
definitions/resources and the item presentation pack, and expose all 21 catalog
items. The client shows tool names and icons in the HUD
(`crates/client/src/item_ui.rs`) and draws held, dropped and projectile items
(`crates/client/src/world_items.rs`). Brick item spawns, contact pickups,
respawn and drops are in `crates/sim/src/session/items.rs` (see
`runtime-world-items.md`). Health, damage, death and minigame rules are in
`crates/sim/src/session/combat.rs`. Dedicated world saves do not include weapon
runtime state.
