# Authoritative world items

Implemented in `crates/sim/src/item_spawners.rs` and `session/items.rs`, with
offline authored bounds from item-presentation-pack-003.

## Native data and startup

All 21 vanilla ItemData choices have original DTS24 header bounds, transformed
offline to native Y-up coordinates. The runtime reads native JSON only. The
shared startup loader verifies presentation/physics checksums, exact item/model
bounds consistency, native model/texture hashes and resource budgets. Original
source checksums are provenance, not checksums of converted JSON. Content
identity9 includes the presentation and physics pack. Client hosting and the
dedicated server install the same bounds before accepting connections.

Source evidence and startup measurements are in
`research/item-spawners/pickup-semantics.md`,
`research/item-rendering/README.md` and
`research/item-spawners/item-physics-startup.md`.

## Host behavior

- Brick item position selectors use world axes; brick quarter turns change its
  extents, not the selector direction. Placement compensates for asymmetric
  model pivots using rotated authored bounds.
- Pickups require overlap with the current authoritative player contact box.
  There is no remote Pickup command or client-supplied pickup position.
  AABB contact without a sight ray is supported by the pinned engine-family
  source; exact closed v20 engine parity remains unverified.
- Outside minigames, ordinary collection does not impose building trust. The
  recovered script defaults to allow unless miniGameCanUse returns zero.
- Ordinary items fill the first empty slot and reject duplicates/full inventory.
  Sports items use the existing separate held-ball runtime, without consuming
  an inventory slot. Their complete movement/minigame/UI adapters remain work.
- Successful static pickup starts the brick's millisecond-configured respawn
  clock, rounded up to a120Hz tick. Full/duplicate failures leave it available.
  Updating direction/position/respawn properties preserves an already-running
  timer (`setItemDirection`, `setItemPosition`, `setItemRespawntime` move or
  retime the same Item). Setting an item, which the wrench's Send always does
  (`IDB`) and the `setItem` event does, runs `fxDTSBrick::setItem`: the old
  Item is deleted and a fresh, available one created. Removing its brick
  removes the transient item.
- DropTool resolves the connection's inventory and current host pose, including
  commands before the first player tick. v20 script throw position is feet plus
  1.5 times vertical scale plus eye direction, velocity20 times scale, without
  inherited player velocity. Drops expire after10 seconds. Rotation uses body
  yaw rather than eye pitch; scale is retained.
- The thrower exclusion is58 native ticks: round-up of the engine-family
  15×32ms=480ms timeout. This duration is not proven from the closed v20 binary.
  Other players can collect immediately. Saved legacy state retains its existing
  lifetime; new weapon saves use schema3, with read compatibility for1/2.
- Static and dynamic contact queries use eight-unit spatial buckets; shapes
  spanning more than128 buckets use an occupied-shape fallback. Negative axes
  and contacts exactly on bucket boundaries are included. Connection order
  deterministically resolves simultaneous collection.
-4096 static items and1024 dropped items are explicit current runtime limits.
  Property edits and build appends preflight static capacity before mutating
  the world. These limits are a measured-envelope task, not unlimited capacity.

Wire protocol8 and Session snapshot5 replicate static items and availability
ticks, dropped-item rotation/scale, image states and inventory. Bricks and
runtime entities have separate ID namespaces. Replica validation rejects
duplicate item identities and invalid rotations/scales.

## Evidence and known gaps

Five headless item tests cover all six placement selectors/four item directions,
asymmetric pivots and rotated bricks, respawn/duplicate/trust behavior, immediate
other-player drop pickup, negative/boundary/oversized broadphase, and transactional
capacity checks. A weapon test covers body orientation/scale, exact58-tick
exclusion and that weapon saves load only at the current save schema (no
migration from earlier ones). Real QUIC verifies dropped inventory
and item state reach another peer and a late join.

The client draws static, dropped, mounted and projectile items
(`crates/client/src/world_items.rs`). Not yet complete: original muzzle poses,
full inventory persistence across host restart, item fake-kill coupling, source-derived drop collision
shape/gravity/friction/elasticity, and item spin presentation. Current drop
motion still uses the earlier provisional point sweep and damping.

A picked-up brick item waits for its respawn as a ghost (2026-09-28).
`Item::fadeOut` sets node colour `<ItemData colorShiftColor rgb or white> 0.25`
and `canPickup = 0`; `fadeIn` restores the image colour after the brick's
respawn time. The script also calls `startFade(0, 0, 1)`, which in the TGE
family (`openmbu-reference/mbg-shapeBase.cc` `startFade`/`advanceTime`) would
hide the shape outright; Maxwell's own v20 observation is that the ghost stays
visible, so the node colour decides. Only the availability tick is replicated;
each client draws the ghost from it at alpha 0.25 with the ItemData colour,
translucent and without a shadow (`world_items.rs` `RESPAWN_GHOST_ALPHA`).
